import SwiftUI
import UniformTypeIdentifiers

private enum ForgeLane: String, CaseIterable, Identifiable {
    case write = "Write"
    case preview = "Preview"
    case outline = "Outline"
    case inspect = "Inspect"
    var id: Self { self }
}

private enum AuxiliaryPanel: String, Identifiable {
    case outline = "Outline"
    case inspect = "Press Settings"
    var id: Self { self }
}

private enum TypeScalePresetStep: String, CaseIterable, Identifiable {
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
        guard let idx = all.firstIndex(of: self) else { return self }
        let target = min(all.count - 1, max(0, idx + delta))
        return all[target]
    }
}

struct ForgeView: View {
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    @AppStorage(LabAppearance.storageKey) private var appearance = LabAppearance.dark.rawValue
    @AppStorage(Lab.textScaleStorageKey) private var uiTextScale = Lab.defaultTextScale
    @AppStorage("renderFontScale") private var renderFontScale = 1.0
    @StateObject private var renderer = MarkdownRendererModel()
    @State private var lane: ForgeLane = .write
    @State private var editorFocused = false

    @State private var exportedPdfData: Data?
    @State private var exportedHtmlText: String?
    @State private var exportItemUrl: URL?
    @State private var showShareSheet = false
    @State private var isExporting = false
    @State private var exportStatusMessage: String?
    @State private var showCopiedAlert = false
    @State private var auxiliaryPanel: AuxiliaryPanel?
    @State private var showDocumentLab = false
    @State private var showSourceImporter = false
    @State private var sourceImportError: String?

    init() {
        let requested = ProcessInfo.processInfo.environment["FMD_INITIAL_LANE"]
        _lane = State(initialValue: ForgeLane(rawValue: requested ?? "") ?? .write)
        _showDocumentLab = State(
            initialValue: ProcessInfo.processInfo.environment["FMD_OPEN_DOCUMENT_LAB"] == "1"
        )
    }

    var body: some View {
        forgePresentation
            .onAppear {
                uiTextScale = Lab.clampedTextScale(uiTextScale)
            }
            .onChange(of: uiTextScale) { _, value in
                let clamped = Lab.clampedTextScale(value)
                if clamped != value { uiTextScale = clamped }
            }
            .preferredColorScheme((LabAppearance(rawValue: appearance) ?? .dark).colorScheme)
    }

    private var forgeLayout: some View {
        GeometryReader { geometry in
            ZStack {
                LaboratoryBackground()
                VStack(spacing: 14) {
                    masthead
                    if geometry.size.width >= 1_180 {
                        wideForge
                    } else if geometry.size.width >= 760 {
                        if geometry.size.height > geometry.size.width {
                            portraitTabletForge
                        } else {
                            regularForge
                        }
                    } else {
                        compactForge
                    }
                    footer
                }
                .padding(.horizontal, geometry.size.width >= 820 ? 22 : 14)
                .padding(.top, 12)
            }
        }
    }

    private var forgeRenderObservers: some View {
        forgeLayout
        .onChange(of: renderer.source) { _, _ in
            renderer.scheduleRender()
            renderer.scheduleDraftSave()
        }
        .onChange(of: renderer.fontFamily) { _, _ in
            renderer.renderNow()
            renderer.scheduleDraftSave()
        }
        .onChange(of: renderer.darkMode) { _, _ in
            renderer.renderNow()
            renderer.scheduleDraftSave()
        }
        .onChange(of: renderer.allowRawHtml) { _, _ in renderer.renderNow() }
        .onChange(of: renderer.toc) { _, _ in
            renderer.renderNow()
            renderer.scheduleDraftSave()
        }
        .onChange(of: renderer.tocDepth) { _, _ in
            renderer.renderNow()
            renderer.scheduleDraftSave()
        }
        .onChange(of: renderer.customCSS) { _, _ in
            renderer.renderNow()
            renderer.scheduleDraftSave()
        }
    }

    private var forgeMetadataObservers: some View {
        forgeRenderObservers
        .onChange(of: renderer.language) { _, _ in
            renderer.renderNow()
            renderer.scheduleDraftSave()
        }
        .onChange(of: renderer.documentTitle) { _, _ in
            renderer.renderNow()
            renderer.scheduleDraftSave()
        }
        .onChange(of: renderer.documentAuthor) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.pageNumbers) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.codeLineNumbers) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.microtypeProtrusion) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.fitToPages) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.customizePDFTypography) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.pdfBaseFontSize) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.pdfHeadingScale) { _, _ in renderer.scheduleDraftSave() }
        .onChange(of: renderer.pdfTableFontSize) { _, _ in renderer.scheduleDraftSave() }
    }

    private var forgeModelObservers: some View {
        forgeMetadataObservers
        .onChange(of: renderFontScale) { _, scale in
            renderer.renderFontScale = clampedRenderFontScale(scale)
            renderer.renderNow()
        }
        .onAppear {
            let clamped = clampedRenderFontScale(renderFontScale)
            if renderFontScale != clamped { renderFontScale = clamped }
            renderer.renderFontScale = clamped
        }
        .onReceive(NotificationCenter.default.publisher(for: .renderMarkdownNow)) { _ in
            renderAndRevealPreview()
        }
        .onReceive(NotificationCenter.default.publisher(for: .exportPdfNow)) { _ in
            triggerPdfExport()
        }
        .onReceive(NotificationCenter.default.publisher(for: .exportHtmlNow)) { _ in
            triggerHtmlExport()
        }
    }

    private var forgeDocumentEvents: some View {
        forgeModelObservers
        .onReceive(NotificationCenter.default.publisher(for: .newMarkdownDocument)) { _ in
            newSourceDocument()
        }
        .onReceive(NotificationCenter.default.publisher(for: .openMarkdownDocument)) { _ in
            showSourceImporter = true
        }
        .onOpenURL { url in
            if url.isFileURL {
                loadSourceDocument(from: url)
                return
            }
            guard url.scheme?.lowercased() == "frankenmarkdown" else { return }
            switch url.host?.lowercased() {
            case "lab":
                showDocumentLab = true
            case "publish":
                showDocumentLab = true
            case "write":
                lane = .write
            case "preview":
                lane = .preview
            default:
                break
            }
        }
        .fileImporter(
            isPresented: $showSourceImporter,
            allowedContentTypes: [.plainText],
            allowsMultipleSelection: false,
            onCompletion: importSourceDocument
        )
        .alert("Couldn’t Open Document", isPresented: Binding(
            get: { sourceImportError != nil },
            set: { if !$0 { sourceImportError = nil } }
        )) {
            Button("OK", role: .cancel) { sourceImportError = nil }
        } message: {
            Text(sourceImportError ?? "The selected document could not be opened.")
        }
    }

    private var forgePresentation: some View {
        forgeDocumentEvents
        .sheet(isPresented: $showShareSheet) {
            if let url = exportItemUrl {
                ShareActivityView(fileURL: url)
            }
        }
        .sheet(item: $auxiliaryPanel) { panel in
            NavigationStack {
                ZStack {
                    LaboratoryBackground()
                    Group {
                        switch panel {
                        case .outline: outlinePanel
                        case .inspect: inspectorPanel
                        }
                    }
                    .padding(16)
                }
                .navigationTitle(panel.rawValue)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { auxiliaryPanel = nil }
                    }
                }
            }
        }
        .fullScreenCover(isPresented: $showDocumentLab) {
            DocumentLabView(renderer: renderer)
        }
        .overlay(alignment: .top) {
            if showCopiedAlert {
                HStack(spacing: 8) {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(Lab.emerald)
                    Text("Copied to clipboard")
                        .font(.system(size: Lab.size(12), weight: .bold, design: .monospaced))
                        .foregroundStyle(Lab.text)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Lab.panelStrong, in: Capsule())
                .overlay(Capsule().stroke(Lab.emerald.opacity(0.4)))
                .padding(.top, 16)
                .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
    }

    private var masthead: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) { brand; Spacer(); actionButtons; LabAppearanceButton(selection: $appearance); statusPill }
            VStack(alignment: .leading, spacing: 10) {
                HStack { brand; Spacer(); LabAppearanceButton(selection: $appearance); statusPill }
                actionButtons
            }
        }
    }

    private var brand: some View {
        HStack(spacing: 12) {
            Image("MonsterIcon")
                .resizable()
                .scaledToFill()
                .frame(width: 52, height: 52)
                .clipShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
                .shadow(color: Lab.emerald.opacity(0.42), radius: 13)
                .accessibilityLabel("Friendly FrankenMarkdown document monster")
            VStack(alignment: .leading, spacing: 1) {
                FrankenWordmark(
                    productInitial: "M",
                    productRemainder: "ARKDOWN",
                    fullName: "FrankenMarkdown"
                )
                Text("DOCUMENT_PRESS // private · offline · Rust")
                    .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                    .foregroundStyle(Lab.secondary)
            }
        }
    }

    private var actionButtons: some View {
        HStack(spacing: 8) {
            Menu {
                Button {
                    showSourceImporter = true
                } label: {
                    Label("Open Markdown…", systemImage: "folder")
                }
                Button {
                    newSourceDocument()
                } label: {
                    Label("New Document", systemImage: "doc.badge.plus")
                }
                Divider()
                ForEach(MarkdownRendererModel.presets) { preset in
                    Button {
                        renderer.source = preset.markdown
                    } label: {
                        Label(preset.title, systemImage: "doc.text")
                    }
                }
            } label: {
                Label("Document", systemImage: "doc.text")
                    .font(.system(size: Lab.size(11), weight: .bold, design: .monospaced))
                    .foregroundStyle(Lab.text)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                    .background(Lab.panelStrong, in: RoundedRectangle(cornerRadius: 8))
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(Lab.stroke))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }

            Button {
                showDocumentLab = true
            } label: {
                Label("Document Lab", systemImage: "bolt.horizontal.circle.fill")
                    .font(.system(size: Lab.size(11), weight: .black, design: .monospaced))
                    .foregroundStyle(Lab.onEmerald)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .background(Lab.emerald, in: Capsule())
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }

            if horizontalSizeClass == .regular {
#if targetEnvironment(macCatalyst)
                Button {
                    auxiliaryPanel = .outline
                } label: {
                    Label("Outline", systemImage: "list.bullet.indent")
                        .font(.system(size: Lab.size(11), weight: .bold, design: .monospaced))
                        .foregroundStyle(Lab.text)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .background(Lab.panelStrong, in: RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Lab.stroke))
                        .lineLimit(1)
                        .fixedSize(horizontal: true, vertical: false)
                }
#else
                Menu {
                    Button {
                        auxiliaryPanel = .outline
                    } label: {
                        Label("Document Outline", systemImage: "list.bullet.indent")
                    }
                    Button {
                        auxiliaryPanel = .inspect
                    } label: {
                        Label("Press Settings", systemImage: "slider.horizontal.3")
                    }
                } label: {
                    Label("Tools", systemImage: "wrench.and.screwdriver")
                        .font(.system(size: Lab.size(11), weight: .bold, design: .monospaced))
                        .foregroundStyle(Lab.text)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .background(Lab.panelStrong, in: RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Lab.stroke))
                }
#endif
            }

            Menu {
                Button { triggerPdfExport() } label: {
                    Label("PDF document", systemImage: "doc.richtext")
                }
                Button { triggerHtmlExport() } label: {
                    Label("Self-contained HTML", systemImage: "globe")
                }
                Divider()
                Button { showDocumentLab = true } label: {
                    Label("All publishing formats…", systemImage: "sparkles.rectangle.stack")
                }
            } label: {
                Label("Publish", systemImage: "arrow.up.doc")
                    .font(.system(size: Lab.size(11), weight: .bold, design: .monospaced))
                    .foregroundStyle(Lab.text)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                    .background(Lab.panelStrong, in: RoundedRectangle(cornerRadius: 8))
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(Lab.stroke))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }
            .disabled(isExporting)
        }
    }

    private var statusPill: some View {
        HStack(spacing: 8) {
            Image(systemName: statusSymbol)
            Text(statusText)
                .lineLimit(1)
            if renderer.phase == .rendering || isExporting { ProgressView().controlSize(.small) }
        }
        .font(.system(size: Lab.size(10), weight: .bold, design: .monospaced))
        .foregroundStyle(statusColor)
        .padding(.horizontal, 13)
        .padding(.vertical, 9)
        .background(Lab.panelStrong, in: Capsule())
        .overlay(Capsule().stroke(statusColor.opacity(0.28)))
        .fixedSize(horizontal: true, vertical: false)
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
            case .outline: outlinePanel
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
            inspectorPanel
                .frame(width: 260)
        }
    }

    private var regularForge: some View {
        HStack(spacing: 14) {
            editorPanel
                .frame(minWidth: 320, maxWidth: .infinity)
            previewPanel
                .frame(minWidth: 360, maxWidth: .infinity)
        }
    }

    private var portraitTabletForge: some View {
        VStack(spacing: 14) {
            editorPanel
                .frame(minHeight: 320, maxHeight: .infinity)
            previewPanel
                .frame(minHeight: 320, maxHeight: .infinity)
        }
    }

    private var editorPanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                LabLabel(text: "01 · The Source")
                Spacer()
                Label(renderer.draftStatus, systemImage: "externaldrive.badge.checkmark")
                    .font(.system(size: Lab.size(9), design: .monospaced))
                    .foregroundStyle(Lab.emerald)
                    .accessibilityHint("The active draft stays on this device")
                Text("\(renderer.source.utf8.count) bytes · \(characterCount) chars · \(wordCount) words")
                        .font(.system(size: Lab.size(9), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
                MarkdownCodeEditor(text: $renderer.source, isFocused: $editorFocused)
                    .background(Lab.panelStrong, in: RoundedRectangle(cornerRadius: 12))
                    .frame(minHeight: 320)
#if !targetEnvironment(macCatalyst)
                if horizontalSizeClass == .compact {
                    HStack {
                        Button {
                            renderAndRevealPreview()
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
#endif
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

    private func renderAndRevealPreview() {
        editorFocused = false
        renderer.renderNow()
        withAnimation(.snappy) { lane = .preview }
    }

    private var outlinePanel: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                LabLabel(text: "Document Outline")
                if renderer.headings.isEmpty {
                    Text("No headings found in source Markdown (# Heading).")
                        .font(.system(size: Lab.size(12)))
                        .foregroundStyle(Lab.secondary)
                        .padding(.vertical, 16)
                } else {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(renderer.headings) { heading in
                                HStack(spacing: 8) {
                                    Text(String(repeating: "· ", count: heading.level - 1) + "H\(heading.level)")
                                        .font(.system(size: Lab.size(10), weight: .black, design: .monospaced))
                                        .foregroundStyle(Lab.emerald)
                                    Text(heading.title)
                                        .font(.system(size: Lab.size(12), weight: .medium))
                                        .foregroundStyle(Lab.text)
                                        .lineLimit(1)
                                    Spacer()
                                    Text("L\(heading.lineNumber)")
                                        .font(.system(size: Lab.size(9), design: .monospaced))
                                        .foregroundStyle(Lab.secondary)
                                }
                                .padding(.vertical, 4)
                            }
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var inspectorPanel: some View {
        LabPanel {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    LabLabel(text: "03 · The Press")

                    VStack(alignment: .leading, spacing: 6) {
                        Text("FONT FAMILY")
                            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                        Picker("Font", selection: $renderer.fontFamily) {
                            Text("Sans (IBM Plex)").tag("sans")
                            Text("Serif (CM Serif)").tag("serif")
                        }
                        .pickerStyle(.segmented)
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("RENDERED TEXT SIZE")
                            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                        renderFontSizeControl
                        Text("Changes the reading view and exported document—not the editor or app controls.")
                            .font(.system(size: Lab.size(9)))
                            .foregroundStyle(Lab.secondary)
                    }

                    VStack(alignment: .leading, spacing: 6) {
                        Text("COLOR THEME")
                            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                        Picker("Theme", selection: $renderer.darkMode) {
                            Text("Adaptive Dark").tag("auto")
                            Text("Light").tag("disabled")
                        }
                        .pickerStyle(.segmented)
                    }

                    Divider().background(Lab.stroke)

                    VStack(alignment: .leading, spacing: 8) {
                        Text("DOCUMENT SYSTEM")
                            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                        TextField("Title", text: $renderer.documentTitle)
                            .textFieldStyle(.roundedBorder)
                        TextField("Author", text: $renderer.documentAuthor)
                            .textFieldStyle(.roundedBorder)
                        Picker("Language", selection: $renderer.language) {
                            Text("English").tag("en")
                            Text("Deutsch").tag("de")
                            Text("Français").tag("fr")
                            Text("Español").tag("es")
                            Text("Nederlands").tag("nl")
                        }
                        Toggle("Table of contents", isOn: $renderer.toc)
                        if renderer.toc {
                            Stepper("Contents depth: H\(renderer.tocDepth)", value: $renderer.tocDepth, in: 1...6)
                        }
                    }
                    .font(.system(size: Lab.size(11)))
                    .foregroundStyle(Lab.text)

                    Divider().background(Lab.stroke)

                    VStack(alignment: .leading, spacing: 8) {
                        Toggle("PDF Page Numbers", isOn: $renderer.pageNumbers)
                        Toggle("Code Line Numbers", isOn: $renderer.codeLineNumbers)
                        Toggle("Optical-margin microtype", isOn: $renderer.microtypeProtrusion)
                        Toggle("Allow Raw HTML", isOn: $renderer.allowRawHtml)
                    }
                    .font(.system(size: Lab.size(12)))
                    .foregroundStyle(Lab.text)

                    Divider().background(Lab.stroke)

                    VStack(alignment: .leading, spacing: 6) {
                        Label("Raw HTML off by default", systemImage: "lock.shield")
                        Label("Offline on-device Rust core", systemImage: "network.slash")
                        Label("Exact checked WASM package", systemImage: "shippingbox")
                    }
                    .font(.system(size: Lab.size(11)))
                    .foregroundStyle(Lab.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
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

    private var renderFontSizeControl: some View {
        let currentStep = TypeScalePresetStep.closest(to: renderFontScale)
        return HStack(spacing: 8) {
            Button {
                let nextStep = currentStep.next(delta: -1)
                renderFontScale = nextStep.scale
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
                let nextStep = currentStep.next(delta: 1)
                renderFontScale = nextStep.scale
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

    private func clampedRenderFontScale(_ value: Double) -> Double {
        TypeScalePresetStep.closest(to: value).scale
    }

    private var characterCount: Int {
        renderer.source.count
    }

    private var wordCount: Int {
        let components = renderer.source.components(separatedBy: .whitespacesAndNewlines)
        return components.filter { !$0.isEmpty }.count
    }

    private var statusText: String {
        switch renderer.phase {
        case .loading: "warming the document press"
        case .ready: "Rust press ready"
        case .rendering: "parse · theme · layout · render"
        case .exporting(let msg): msg
        case .failed(let message): message
        }
    }

    private var statusSymbol: String {
        switch renderer.phase {
        case .loading: "bolt.horizontal.circle"
        case .ready: "checkmark.seal"
        case .rendering: "gearshape.2"
        case .exporting: "arrow.down.circle"
        case .failed: "exclamationmark.triangle"
        }
    }

    private var statusColor: Color {
        switch renderer.phase {
        case .loading, .rendering, .exporting: Lab.amber
        case .ready: Lab.emerald
        case .failed: Lab.danger
        }
    }

    private func exportFilename(ext: String) -> String {
        let trimmed = renderer.documentTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            return "Document.\(ext)"
        }
        let safe = trimmed.components(separatedBy: CharacterSet(charactersIn: "/:\\?%*|\"<>")).joined(separator: "-")
        return "\(safe).\(ext)"
    }

    private func triggerPdfExport() {
        guard !isExporting else { return }
        isExporting = true
        Task {
            do {
                let (data, _, _) = try await renderer.exportPdf()
                let tempDir = FileManager.default.temporaryDirectory
                let fileUrl = tempDir.appendingPathComponent(exportFilename(ext: "pdf"))
                try data.write(to: fileUrl)
                exportItemUrl = fileUrl
                showShareSheet = true
                isExporting = false
            } catch {
                isExporting = false
            }
        }
    }

    private func triggerHtmlExport() {
        guard !isExporting else { return }
        isExporting = true
        Task {
            do {
                let (html, _, _) = try await renderer.exportHtml()
                let tempDir = FileManager.default.temporaryDirectory
                let fileUrl = tempDir.appendingPathComponent(exportFilename(ext: "html"))
                try html.write(to: fileUrl, atomically: true, encoding: .utf8)
                exportItemUrl = fileUrl
                showShareSheet = true
                isExporting = false
            } catch {
                isExporting = false
            }
        }
    }

    private func importSourceDocument(_ result: Result<[URL], Error>) {
        do {
            guard let url = try result.get().first else { return }
            loadSourceDocument(from: url)
        } catch {
            sourceImportError = error.localizedDescription
        }
    }

    private func newSourceDocument() {
        renderer.source = "# New Document\n\nStart writing..."
        renderer.documentTitle = ""
        lane = .write
        sourceImportError = nil
    }

    private func loadSourceDocument(from url: URL) {
        Task {
            do {
                let document = try await Task.detached(priority: .userInitiated) {
                    try MarkdownSourceLoader.load(from: url)
                }.value
                renderer.source = document.source
                renderer.documentTitle = document.suggestedTitle
                lane = .write
                sourceImportError = nil
            } catch {
                sourceImportError = error.localizedDescription
            }
        }
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
            // Register only a copied file representation. A bare HTML URL can
            // otherwise be interpreted as a web link or text by destinations.
            completion(fileURL, false, nil)
            return nil
        }
        let configuration = UIActivityItemsConfiguration(itemProviders: [provider])
        return UIActivityViewController(activityItemsConfiguration: configuration)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
