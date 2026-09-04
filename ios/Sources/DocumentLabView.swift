import SwiftUI
import UniformTypeIdentifiers
import WebKit

private enum DocumentForgeRoute: String, CaseIterable, Hashable, Identifiable {
    case intelligence
    case publish
    case compare
    case collection
    case templates
    case settings

    var id: Self { self }

    var title: String {
        switch self {
        case .intelligence: "Inspect & Perfect"
        case .publish: "Publish Everywhere"
        case .compare: "Compare Meaning"
        case .collection: "Build a Collection"
        case .templates: "Start Brilliantly"
        case .settings: "Editorial Studio"
        }
    }

    var eyebrow: String {
        switch self {
        case .intelligence: "DOCUMENT INTELLIGENCE"
        case .publish: "SIX NATIVE OUTPUTS"
        case .compare: "SEMANTIC TIME MACHINE"
        case .collection: "BOOK + SITE WORKFLOW"
        case .templates: "EDITORIAL SYSTEMS"
        case .settings: "TYPE + LANGUAGE + PRINT"
        }
    }

    var detail: String {
        switch self {
        case .intelligence: "Readability, structure, accessibility, links, and search anatomy—mapped locally."
        case .publish: "Turn one source into PDF, HTML, SVG, EPUB, a living workspace, or search JSON."
        case .compare: "See what changed in the document’s meaning, not merely which lines moved."
        case .collection: "Bind folders, includes, navigation, search, and chapters into a complete publication."
        case .templates: "Begin with a sophisticated report, research paper, launch plan, book, or diagram atlas."
        case .settings: "Shape the reading experience with language-aware typography and precise print controls."
        }
    }

    var symbol: String {
        switch self {
        case .intelligence: "waveform.path.ecg.rectangle.fill"
        case .publish: "sparkles.rectangle.stack.fill"
        case .compare: "arrow.trianglehead.branch"
        case .collection: "books.vertical.fill"
        case .templates: "rectangle.stack.badge.plus"
        case .settings: "slider.horizontal.3"
        }
    }

    var tint: Color {
        switch self {
        case .intelligence: Lab.emerald
        case .publish: Lab.cyan
        case .compare: Lab.amber
        case .collection: Color(red: 0.67, green: 0.48, blue: 1.0)
        case .templates: Color(red: 1.0, green: 0.42, blue: 0.72)
        case .settings: Color(red: 0.46, green: 0.68, blue: 1.0)
        }
    }

    var badge: String {
        switch self {
        case .intelligence: "LIVE"
        case .publish: "6 FORMATS"
        case .compare: "AST DIFF"
        case .collection: "MULTI-FILE"
        case .templates: "8 SYSTEMS"
        case .settings: "PRECISE"
        }
    }
}

struct DocumentLabView: View {
    @ObservedObject var renderer: MarkdownRendererModel
    @Environment(\.dismiss) private var dismiss

    @State private var routePath: [DocumentForgeRoute]
    @State private var isWorking = false
    @State private var errorMessage: String?
    @State private var exportURL: URL?
    @State private var showShare = false
    @State private var baseline = ""
    @State private var diffPreview: SemanticDiffPreview?
    @State private var showBaselineImporter = false
    @State private var showBookImporter = false
    @State private var showStylesheetImporter = false
    @State private var bookFiles: [BookSourceFile] = []

    init(renderer: MarkdownRendererModel) {
        self.renderer = renderer
        let requested = ProcessInfo.processInfo.environment["FMD_FORGE_ROUTE"]
        if let requested, let route = DocumentForgeRoute(rawValue: requested) {
            _routePath = State(initialValue: [route])
        } else {
            _routePath = State(initialValue: [])
        }
    }

    var body: some View {
        NavigationStack(path: $routePath) {
            GeometryReader { geometry in
                ZStack {
                    LaboratoryBackground()
                    home(width: geometry.size.width)
                }
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(Lab.panelStrong, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    HStack(spacing: 8) {
                        Image(systemName: "bolt.horizontal.circle.fill")
                            .foregroundStyle(Lab.emerald)
                        Text("DOCUMENT FORGE")
                            .font(.system(size: Lab.size(12), weight: .black, design: .monospaced))
                            .foregroundStyle(Lab.text)
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .fontWeight(.bold)
                }
            }
            .navigationDestination(for: DocumentForgeRoute.self) { route in
                destination(route)
            }
        }
        .sheet(isPresented: $showShare) {
            if let exportURL { ShareActivityView(fileURL: exportURL) }
        }
        .fileImporter(
            isPresented: $showBaselineImporter,
            allowedContentTypes: [.plainText],
            allowsMultipleSelection: false,
            onCompletion: importBaseline
        )
        .fileImporter(
            isPresented: $showBookImporter,
            allowedContentTypes: [.folder, .plainText],
            allowsMultipleSelection: true,
            onCompletion: importBookFiles
        )
        .fileImporter(
            isPresented: $showStylesheetImporter,
            allowedContentTypes: [UTType(filenameExtension: "css") ?? .plainText, .plainText],
            allowsMultipleSelection: false,
            onCompletion: importStylesheet
        )
        .safeAreaInset(edge: .top, spacing: 0) {
            if let errorMessage {
                errorBanner(errorMessage)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if isWorking {
                processingBanner
                    .padding(.horizontal, 16)
                    .padding(.bottom, 8)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .task {
            if renderer.analysisIsStale { await refreshAnalysis() }
        }
    }

    private func home(width: CGFloat) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: width < 600 ? 18 : 24) {
                forgeHero(width: width)
                documentPulse

                VStack(alignment: .leading, spacing: 6) {
                    Text("WHAT DO YOU WANT TO MAKE?")
                        .font(.system(size: Lab.size(11), weight: .black, design: .monospaced))
                        .tracking(1.6)
                        .foregroundStyle(Lab.emerald)
                    Text("Choose an outcome. The forge brings the right tools into focus.")
                        .font(.system(size: Lab.size(width < 600 ? 14 : 16), weight: .medium))
                        .foregroundStyle(Lab.secondary)
                }

                LazyVGrid(columns: routeColumns(width: width), spacing: 14) {
                    ForEach(DocumentForgeRoute.allCases) { route in
                        forgePortal(route, compact: width < 600)
                    }
                }

                capabilityRibbon
            }
            .frame(maxWidth: 1_360, alignment: .leading)
            .padding(.horizontal, width < 600 ? 16 : 28)
            .padding(.vertical, width < 600 ? 16 : 28)
            .frame(maxWidth: .infinity)
        }
        .scrollIndicators(.hidden)
        .accessibilityIdentifier("document-forge-home")
    }

    private func routeColumns(width: CGFloat) -> [GridItem] {
        if width >= 1_150 {
            return Array(repeating: GridItem(.flexible(), spacing: 14), count: 3)
        }
        if width >= 680 {
            return Array(repeating: GridItem(.flexible(), spacing: 14), count: 2)
        }
        return [GridItem(.flexible())]
    }

    private func forgeHero(width: CGFloat) -> some View {
        ZStack(alignment: .leading) {
            RoundedRectangle(cornerRadius: 28, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [
                            Color(red: 0.02, green: 0.17, blue: 0.11),
                            Color(red: 0.015, green: 0.09, blue: 0.065),
                            Color(red: 0.03, green: 0.08, blue: 0.14)
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 28, style: .continuous)
                        .stroke(
                            LinearGradient(
                                colors: [Lab.emerald.opacity(0.72), Lab.cyan.opacity(0.18), .clear],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            ),
                            lineWidth: 1
                        )
                )

            ForgeCoreVisual(active: isWorking)
                .allowsHitTesting(false)

            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    Label("PURE RUST", systemImage: "gearshape.2.fill")
                    Text("•")
                    Label("PRIVATE", systemImage: "network.slash")
                    Text("•")
                    Text("ONE SOURCE")
                }
                .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                .foregroundStyle(Lab.emerald)

                Text("One document.\nEvery form.")
                    .font(.system(size: Lab.size(width < 600 ? 35 : 54), weight: .black, design: .rounded))
                    .tracking(-1.2)
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)

                Text("Inspect its structure, perfect its language, compare its meaning, then publish it as anything—without your work leaving this device.")
                    .font(.system(size: Lab.size(width < 600 ? 14 : 17), weight: .medium))
                    .foregroundStyle(Color.white.opacity(0.78))
                    .frame(maxWidth: width < 600 ? .infinity : 620, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(width < 600 ? 22 : 34)
        }
        .frame(minHeight: width < 600 ? 300 : 330)
        .shadow(color: Lab.emerald.opacity(0.16), radius: 28, y: 12)
        .accessibilityElement(children: .combine)
    }

    private var documentPulse: some View {
        Group {
            if let analysis = renderer.analysis {
                NavigationLink(value: DocumentForgeRoute.intelligence) {
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 18) {
                            healthOrb(analysis)
                            analysisSummary(analysis)
                            Spacer(minLength: 12)
                            Image(systemName: "arrow.up.right")
                                .font(.system(size: 18, weight: .black))
                                .foregroundStyle(Lab.emerald)
                        }
                        VStack(alignment: .leading, spacing: 14) {
                            HStack { healthOrb(analysis); analysisSummary(analysis) }
                            Label("Open full document intelligence", systemImage: "arrow.right.circle.fill")
                                .font(.system(size: Lab.size(11), weight: .bold))
                                .foregroundStyle(Lab.emerald)
                        }
                    }
                    .padding(18)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 20).stroke(Lab.emerald.opacity(0.22)))
                }
                .buttonStyle(.plain)
            } else {
                HStack(spacing: 14) {
                    ProgressView().tint(Lab.emerald).controlSize(.large)
                    VStack(alignment: .leading, spacing: 3) {
                        Text("READING THE DOCUMENT’S DNA")
                            .font(.system(size: Lab.size(10), weight: .black, design: .monospaced))
                            .foregroundStyle(Lab.emerald)
                        Text("Mapping prose, structure, accessibility, anchors, and search…")
                            .font(.system(size: Lab.size(13), weight: .medium))
                            .foregroundStyle(Lab.secondary)
                    }
                }
                .padding(18)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
            }
        }
    }

    private func healthOrb(_ analysis: DocumentAnalysis) -> some View {
        let findings = analysis.stats.findings.count + analysis.audit.findings.count
        let tint = findings == 0 ? Lab.emerald : Lab.amber
        return ZStack {
            Circle().stroke(tint.opacity(0.17), lineWidth: 8)
            Circle()
                .trim(from: 0, to: max(0.08, min(1, analysis.stats.fleschReadingEase / 100)))
                .stroke(tint, style: StrokeStyle(lineWidth: 8, lineCap: .round))
                .rotationEffect(.degrees(-90))
            VStack(spacing: 0) {
                Text("\(Int(analysis.stats.fleschReadingEase.rounded()))")
                    .font(.system(size: Lab.size(19), weight: .black, design: .rounded))
                    .foregroundStyle(Lab.text)
                Text(findings == 0 ? "CLEAR" : "\(findings) NOTES")
                    .font(.system(size: Lab.size(7), weight: .black, design: .monospaced))
                    .foregroundStyle(tint)
            }
        }
        .frame(width: 76, height: 76)
    }

    private func analysisSummary(_ analysis: DocumentAnalysis) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text("YOUR DOCUMENT, ALIVE")
                .font(.system(size: Lab.size(10), weight: .black, design: .monospaced))
                .foregroundStyle(Lab.emerald)
            Text("\(analysis.stats.words) words · \(duration(analysis.stats.readingTimeSeconds)) read · \(analysis.stats.structure.headingsTotal) sections")
                .font(.system(size: Lab.size(17), weight: .black, design: .rounded))
                .foregroundStyle(Lab.text)
            Text(analysis.stats.readingEaseLabel)
                .font(.system(size: Lab.size(11), weight: .medium))
                .foregroundStyle(Lab.secondary)
                .lineLimit(2)
        }
    }

    private func forgePortal(_ route: DocumentForgeRoute, compact: Bool) -> some View {
        NavigationLink(value: route) {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    ZStack {
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(route.tint.opacity(0.13))
                        Image(systemName: route.symbol)
                            .font(.system(size: 25, weight: .bold))
                            .foregroundStyle(route.tint)
                    }
                    .frame(width: 52, height: 52)
                    Spacer()
                    Text(route.badge)
                        .font(.system(size: Lab.size(8), weight: .black, design: .monospaced))
                        .foregroundStyle(route.tint)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 5)
                        .background(route.tint.opacity(0.1), in: Capsule())
                }

                VStack(alignment: .leading, spacing: 5) {
                    Text(route.eyebrow)
                        .font(.system(size: Lab.size(8), weight: .black, design: .monospaced))
                        .tracking(1.2)
                        .foregroundStyle(route.tint)
                    Text(route.title)
                        .font(.system(size: Lab.size(20), weight: .black, design: .rounded))
                        .foregroundStyle(Lab.text)
                    Text(route.detail)
                        .font(.system(size: Lab.size(12), weight: .medium))
                        .foregroundStyle(Lab.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                HStack {
                    Text("ENTER")
                        .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                    Spacer()
                    Image(systemName: "arrow.right")
                        .fontWeight(.black)
                }
                .foregroundStyle(route.tint)
            }
            .padding(18)
            .frame(maxWidth: .infinity, minHeight: compact ? 198 : 220, alignment: .leading)
            .background(
                LinearGradient(
                    colors: [route.tint.opacity(0.085), Lab.panel],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                ),
                in: RoundedRectangle(cornerRadius: 22, style: .continuous)
            )
            .overlay(RoundedRectangle(cornerRadius: 22).stroke(route.tint.opacity(0.2)))
            .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("document-forge-route-\(route.rawValue)")
    }

    private var capabilityRibbon: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("THE ENGINE UNDER THE GLASS")
                .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                .foregroundStyle(Lab.secondary)
            FlowLayout(spacing: 8) {
                ForEach([
                    "SEMANTIC AST", "MATHML", "MERMAID", "MICROTYPE", "EPUB 3",
                    "PDF/A", "A11Y AUDIT", "SEARCH INDEX", "TRANSCLUSION", "WOFF FONTS"
                ], id: \.self) { capability in
                    Text(capability)
                        .font(.system(size: Lab.size(8), weight: .black, design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                        .padding(.horizontal, 9)
                        .padding(.vertical, 6)
                        .background(Lab.panelSoft, in: Capsule())
                        .overlay(Capsule().stroke(Lab.stroke))
                }
            }
        }
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private func destination(_ route: DocumentForgeRoute) -> some View {
        ZStack {
            LaboratoryBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    routeHeader(route)
                    switch route {
                    case .intelligence: intelligenceStudio
                    case .publish: publishingStudio
                    case .compare: comparisonStudio
                    case .collection: collectionStudio
                    case .templates: templateStudio
                    case .settings: editorialStudio
                    }
                }
                .frame(maxWidth: 1_180, alignment: .leading)
                .padding(.horizontal, 18)
                .padding(.vertical, 22)
                .frame(maxWidth: .infinity)
            }
            .scrollIndicators(.hidden)
            .accessibilityIdentifier("document-forge-route-scroll-\(route.rawValue)")
        }
        .navigationTitle(route.title)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func routeHeader(_ route: DocumentForgeRoute) -> some View {
        HStack(alignment: .top, spacing: 16) {
            ZStack {
                RoundedRectangle(cornerRadius: 18, style: .continuous).fill(route.tint.opacity(0.14))
                Image(systemName: route.symbol)
                    .font(.system(size: 30, weight: .bold))
                    .foregroundStyle(route.tint)
            }
            .frame(width: 68, height: 68)
            VStack(alignment: .leading, spacing: 5) {
                Text(route.eyebrow)
                    .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                    .tracking(1.4)
                    .foregroundStyle(route.tint)
                Text(route.title)
                    .font(.system(size: Lab.size(29), weight: .black, design: .rounded))
                    .foregroundStyle(Lab.text)
                Text(route.detail)
                    .font(.system(size: Lab.size(13), weight: .medium))
                    .foregroundStyle(Lab.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .padding(.bottom, 4)
    }

    private var intelligenceStudio: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Spacer()
                Button { Task { await refreshAnalysis() } } label: {
                    Label(renderer.analysisIsStale ? "Refresh analysis" : "Analyze again", systemImage: "arrow.clockwise")
                }
                .buttonStyle(.borderedProminent)
                .tint(Lab.emerald)
                .disabled(isWorking)
            }

            if let analysis = renderer.analysis {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 12)], spacing: 12) {
                    metricCard("WORDS", "\(analysis.stats.words)", "text.word.spacing")
                    metricCard("READ", duration(analysis.stats.readingTimeSeconds), "book.pages")
                    metricCard("SPEAK", duration(analysis.stats.speakingTimeSeconds), "waveform")
                    metricCard("EASE", String(format: "%.0f", analysis.stats.fleschReadingEase), "gauge.with.dots.needle.50percent")
                }
                readabilityPanel(analysis)
                structurePanel(analysis.stats.structure)
                findingsPanel(analysis)
                searchPanel(analysis.search)
            } else {
                loadingPanel("The Rust engine is mapping prose, structure, accessibility, and anchors…")
            }
        }
    }

    private var publishingStudio: some View {
        VStack(alignment: .leading, spacing: 18) {
            outputConstellation
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 250), spacing: 12)], spacing: 12) {
                publishCard("PDF", "Print-perfect, selectable, outlined, and ready for archival workflows", "doc.richtext", Lab.amber) { await shareLegacyPDF() }
                publishCard("Reading HTML", "A self-contained, beautifully typeset reading view", "globe", Lab.cyan) { await shareLegacyHTML() }
                publishCard("Vector Poster", "Every glyph and diagram becomes crisp SVG paths", "scribble.variable", Lab.emerald) { await shareArtifact(.svg) }
                publishCard("EPUB 3", "A standards-native, offline e-book for every reader", "books.vertical", Color.purple) { await shareArtifact(.epub) }
                publishCard("Living Workspace", "Editor, preview, document stats, and print in one portable file", "sparkles.rectangle.stack", Color.orange) { await shareArtifact(.interactiveHTML) }
                publishCard("Search Corpus", "Deterministic, anchored JSON for instant local search", "magnifyingglass.circle", Color.blue) { await shareArtifact(.searchIndex) }
            }
            exportSettings
        }
    }

    private var outputConstellation: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                LabLabel(text: "One semantic tree · six first-class artifacts")
                OutputConstellationAnimation(active: isWorking)
                    .frame(height: 150)
                Text("The document is parsed once. Each exporter receives the same structure, so meaning survives every form.")
                    .font(.system(size: Lab.size(11), weight: .medium))
                    .foregroundStyle(Lab.secondary)
            }
        }
    }

    private var comparisonStudio: some View {
        VStack(alignment: .leading, spacing: 16) {
            LabPanel {
                VStack(alignment: .leading, spacing: 14) {
                    HStack(alignment: .top) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(baseline.isEmpty ? "Give the forge a ‘before’" : "Baseline armed")
                                .font(.system(size: Lab.size(18), weight: .black, design: .rounded))
                                .foregroundStyle(Lab.text)
                            Text(baseline.isEmpty ? "Import an earlier Markdown file or snapshot the current source." : "\(baseline.utf8.count) bytes ready for structural alignment")
                                .font(.system(size: Lab.size(12), weight: .medium))
                                .foregroundStyle(Lab.secondary)
                        }
                        Spacer()
                        Image(systemName: baseline.isEmpty ? "clock.badge.questionmark" : "clock.badge.checkmark.fill")
                            .font(.system(size: 34, weight: .bold))
                            .foregroundStyle(baseline.isEmpty ? Lab.amber : Lab.emerald)
                    }
                    ViewThatFits(in: .horizontal) {
                        HStack { comparisonButtons }
                        VStack(alignment: .leading) { comparisonButtons }
                    }
                }
            }

            if let diffPreview {
                diffMetrics(diffPreview.metrics.stats)
                LabPanel {
                    VStack(alignment: .leading, spacing: 10) {
                        LabLabel(text: "Meaning-level change map")
                        DiffWebView(html: diffPreview.html)
                            .frame(minHeight: 520)
                            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                    }
                }
            } else {
                comparisonEmptyState
            }
        }
    }

    @ViewBuilder
    private var comparisonButtons: some View {
        Button("Import earlier version") { showBaselineImporter = true }
            .buttonStyle(.borderedProminent)
            .tint(Lab.emerald)
        Button("Snapshot current") { baseline = renderer.source; diffPreview = nil }
            .buttonStyle(.bordered)
        Button("Compare meaning") { Task { await runDiff() } }
            .buttonStyle(.borderedProminent)
            .tint(Lab.amber)
            .disabled(baseline.isEmpty || isWorking)
    }

    private var comparisonEmptyState: some View {
        VStack(spacing: 14) {
            Image(systemName: "text.page.badge.magnifyingglass")
                .font(.system(size: 52, weight: .thin))
                .foregroundStyle(Lab.amber)
            Text("Lines are noisy. Meaning is signal.")
                .font(.system(size: Lab.size(22), weight: .black, design: .rounded))
                .foregroundStyle(Lab.text)
            Text("FrankenMarkdown aligns semantic blocks first, then reveals word-level changes only where ideas actually changed.")
                .font(.system(size: Lab.size(13), weight: .medium))
                .foregroundStyle(Lab.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 560)
        }
        .frame(maxWidth: .infinity, minHeight: 280)
        .background(Lab.panel, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22).stroke(Lab.amber.opacity(0.15)))
    }

    private var collectionStudio: some View {
        VStack(alignment: .leading, spacing: 16) {
            LabPanel {
                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            LabLabel(text: "Publication manifest")
                            Text(bookFiles.isEmpty ? "No chapters selected" : "\(bookFiles.count) chapter\(bookFiles.count == 1 ? "" : "s") ready")
                                .font(.system(size: Lab.size(20), weight: .black, design: .rounded))
                                .foregroundStyle(Lab.text)
                        }
                        Spacer()
                        Button(bookFiles.isEmpty ? "Choose folder" : "Replace") { showBookImporter = true }
                            .buttonStyle(.borderedProminent)
                            .tint(Lab.emerald)
                    }

                    if bookFiles.isEmpty {
                        collectionEmptyState
                    } else {
                        VStack(spacing: 0) {
                            ForEach(bookFiles) { file in
                                HStack(spacing: 10) {
                                    Image(systemName: "doc.text.fill").foregroundStyle(Lab.emerald)
                                    Text(file.path)
                                        .font(.system(size: Lab.size(11), weight: .medium, design: .monospaced))
                                        .foregroundStyle(Lab.text)
                                        .lineLimit(1)
                                    Spacer()
                                    Text(ByteCountFormatter.string(fromByteCount: Int64(file.source.utf8.count), countStyle: .file))
                                        .font(.system(size: Lab.size(9), design: .monospaced))
                                        .foregroundStyle(Lab.secondary)
                                }
                                .padding(.vertical, 10)
                                Divider().overlay(Lab.stroke)
                            }
                        }
                    }

                    ViewThatFits(in: .horizontal) {
                        HStack { collectionButtons }
                        VStack(alignment: .leading) { collectionButtons }
                    }
                    .disabled(bookFiles.isEmpty || isWorking)
                }
            }
        }
    }

    @ViewBuilder
    private var collectionButtons: some View {
        Button { Task { await shareBook(.site) } } label: {
            Label("Build complete site ZIP", systemImage: "shippingbox.fill")
        }
        .buttonStyle(.borderedProminent)
        .tint(Lab.cyan)
        Button { Task { await shareBook(.pdf) } } label: {
            Label("Bind continuous PDF", systemImage: "book.closed.fill")
        }
        .buttonStyle(.borderedProminent)
        .tint(Lab.amber)
    }

    private var collectionEmptyState: some View {
        HStack(spacing: 16) {
            Image(systemName: "folder.badge.plus")
                .font(.system(size: 38, weight: .thin))
                .foregroundStyle(Lab.emerald)
            Text("Choose a Markdown folder or several files. Includes, relative links, navigation, search, frontmatter, and chapter order stay inside the private workspace.")
                .font(.system(size: Lab.size(12), weight: .medium))
                .foregroundStyle(Lab.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(16)
        .background(Lab.panelSoft, in: RoundedRectangle(cornerRadius: 15))
    }

    private var templateStudio: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 280), spacing: 14)], spacing: 14) {
            ForEach(MarkdownRendererModel.presets) { preset in
                Button {
                    renderer.source = preset.markdown
                    renderer.documentTitle = preset.title
                    dismiss()
                } label: {
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            ZStack {
                                RoundedRectangle(cornerRadius: 13).fill(templateColor(preset.id).opacity(0.13))
                                Image(systemName: presetSymbol(preset.id))
                                    .font(.system(size: 26, weight: .bold))
                                    .foregroundStyle(templateColor(preset.id))
                            }
                            .frame(width: 52, height: 52)
                            Spacer()
                            Text("\(wordCount(preset.markdown)) WORDS")
                                .font(.system(size: Lab.size(8), weight: .black, design: .monospaced))
                                .foregroundStyle(Lab.secondary)
                        }
                        Text(preset.title)
                            .font(.system(size: Lab.size(19), weight: .black, design: .rounded))
                            .foregroundStyle(Lab.text)
                        Text(preset.description)
                            .font(.system(size: Lab.size(12), weight: .medium))
                            .foregroundStyle(Lab.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 4)
                        Label("Open this editorial system", systemImage: "arrow.right.circle.fill")
                            .font(.system(size: Lab.size(10), weight: .bold, design: .monospaced))
                            .foregroundStyle(templateColor(preset.id))
                    }
                    .padding(18)
                    .frame(maxWidth: .infinity, minHeight: 230, alignment: .leading)
                    .background(
                        LinearGradient(
                            colors: [templateColor(preset.id).opacity(0.08), Lab.panel],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        ),
                        in: RoundedRectangle(cornerRadius: 21, style: .continuous)
                    )
                    .overlay(RoundedRectangle(cornerRadius: 21).stroke(templateColor(preset.id).opacity(0.22)))
                }
                .buttonStyle(.plain)
            }
        }
    }

    private var editorialStudio: some View {
        VStack(alignment: .leading, spacing: 16) {
            LabPanel {
                VStack(alignment: .leading, spacing: 15) {
                    LabLabel(text: "Identity")
                    TextField("Document title", text: $renderer.documentTitle)
                        .textFieldStyle(.roundedBorder)
                    TextField("Author", text: $renderer.documentAuthor)
                        .textFieldStyle(.roundedBorder)
                    Picker("Document language", selection: $renderer.language) {
                        Text("English").tag("en")
                        Text("German").tag("de")
                        Text("French").tag("fr")
                        Text("Spanish").tag("es")
                        Text("Dutch").tag("nl")
                    }
                }
            }
            LabPanel {
                VStack(alignment: .leading, spacing: 15) {
                    LabLabel(text: "Navigation + print")
                    Toggle("Generate a table of contents", isOn: $renderer.toc)
                    if renderer.toc {
                        Stepper("Contents depth: \(renderer.tocDepth)", value: $renderer.tocDepth, in: 1 ... 6)
                    }
                    Toggle("Print page numbers", isOn: $renderer.pageNumbers)
                    Toggle("Code-block line numbers", isOn: $renderer.codeLineNumbers)
                    Toggle("Optical-margin microtype", isOn: $renderer.microtypeProtrusion)
                    Stepper(renderer.fitToPages == 0 ? "Natural page count" : "Fit to \(renderer.fitToPages) pages", value: $renderer.fitToPages, in: 0 ... 20)
                }
                .font(.system(size: Lab.size(13), weight: .medium))
                .foregroundStyle(Lab.text)
            }
            LabPanel {
                VStack(alignment: .leading, spacing: 12) {
                    LabLabel(text: "Web stylesheet")
                    Text(
                        renderer.customCSS.isEmpty
                            ? "Bundled editorial styling is active."
                            : "Custom CSS replaces the complete bundled stylesheet for preview and HTML-family exports."
                    )
                    .font(.system(size: Lab.size(11), weight: .medium))
                    .foregroundStyle(Lab.secondary)
                    HStack {
                        Button {
                            showStylesheetImporter = true
                        } label: {
                            Label(
                                renderer.customCSS.isEmpty ? "Choose CSS…" : "Replace CSS…",
                                systemImage: "paintbrush.pointed.fill"
                            )
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(Lab.emerald)
                        .accessibilityIdentifier("custom-css-import")
                        if !renderer.customCSS.isEmpty {
                            Button("Remove", role: .destructive) { renderer.customCSS = "" }
                                .buttonStyle(.bordered)
                                .accessibilityIdentifier("custom-css-remove")
                            Spacer()
                            Text(ByteCountFormatter.string(
                                fromByteCount: Int64(renderer.customCSS.utf8.count),
                                countStyle: .file
                            ))
                            .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                            .foregroundStyle(Lab.cyan)
                        }
                    }
                    Text("Only UTF-8 text up to 256 KB is accepted. Rendering grants the stylesheet no network access.")
                        .font(.system(size: Lab.size(9), weight: .medium))
                        .foregroundStyle(Lab.secondary)
                }
                .foregroundStyle(Lab.text)
            }
            LabPanel {
                VStack(alignment: .leading, spacing: 15) {
                    HStack(alignment: .firstTextBaseline) {
                        VStack(alignment: .leading, spacing: 3) {
                            LabLabel(text: "PDF type system")
                            Text("Use the Rust press’s precise body, heading, and table controls.")
                                .font(.system(size: Lab.size(11), weight: .medium))
                                .foregroundStyle(Lab.secondary)
                        }
                        Spacer()
                        Toggle("Custom PDF typography", isOn: $renderer.customizePDFTypography)
                            .labelsHidden()
                            .accessibilityIdentifier("custom-pdf-typography-toggle")
                    }
                    if renderer.customizePDFTypography {
                        typographySlider(
                            title: "Body",
                            value: $renderer.pdfBaseFontSize,
                            range: 6 ... 24,
                            step: 0.5,
                            fractionDigits: 1,
                            suffix: "pt"
                        )
                        typographySlider(
                            title: "Heading ratio",
                            value: $renderer.pdfHeadingScale,
                            range: 1.05 ... 2,
                            step: 0.05,
                            fractionDigits: 2,
                            suffix: "×"
                        )
                        typographySlider(
                            title: "Tables",
                            value: Binding(
                                get: { min(renderer.pdfTableFontSize, renderer.pdfBaseFontSize) },
                                set: { renderer.pdfTableFontSize = min($0, renderer.pdfBaseFontSize) }
                            ),
                            range: 5 ... max(5, renderer.pdfBaseFontSize),
                            step: 0.5,
                            fractionDigits: 1,
                            suffix: "pt"
                        )
                        Text("These controls affect single-document PDF exports. Collection books and web formats keep the editorial profile above.")
                            .font(.system(size: Lab.size(10), weight: .medium))
                            .foregroundStyle(Lab.secondary)
                    }
                }
                .font(.system(size: Lab.size(13), weight: .medium))
                .foregroundStyle(Lab.text)
            }
        }
    }

    private func typographySlider(
        title: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double,
        fractionDigits: Int,
        suffix: String
    ) -> some View {
        let formattedValue = value.wrappedValue.formatted(
            .number.precision(.fractionLength(fractionDigits))
        )
        return VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(title)
                Spacer()
                Text("\(formattedValue)\(suffix)")
                    .font(.system(size: Lab.size(10), weight: .black, design: .monospaced))
                    .foregroundStyle(Lab.emerald)
            }
            Slider(value: value, in: range, step: step)
                .tint(Lab.emerald)
                .accessibilityLabel(title)
                .accessibilityValue("\(formattedValue) \(suffix)")
        }
    }

    private var exportSettings: some View {
        LabPanel {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    LabLabel(text: "Editorial profile")
                    Text(editorialProfileSummary)
                        .font(.system(size: Lab.size(12), weight: .medium))
                        .foregroundStyle(Lab.secondary)
                }
                Spacer()
                NavigationLink(value: DocumentForgeRoute.settings) {
                    Label("Adjust", systemImage: "slider.horizontal.3")
                }
                .buttonStyle(.bordered)
                .tint(Lab.emerald)
            }
        }
    }

    private var editorialProfileSummary: String {
        var parts = [renderer.language.uppercased(), renderer.fontFamily.capitalized]
        if renderer.toc { parts.append("contents") }
        if renderer.pageNumbers { parts.append("page numbers") }
        if renderer.microtypeProtrusion { parts.append("microtype") }
        return parts.joined(separator: " · ")
    }

    private func readabilityPanel(_ analysis: DocumentAnalysis) -> some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 15) {
                HStack {
                    LabLabel(text: "Readability spectrum")
                    Spacer()
                    Text(analysis.stats.readingEaseLabel)
                        .font(.system(size: Lab.size(10), weight: .bold))
                        .foregroundStyle(Lab.emerald)
                }
                ReadabilitySpectrum(score: analysis.stats.fleschReadingEase)
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 28) {
                        readabilityScore("FLESCH–KINCAID", analysis.stats.fleschKincaidGrade)
                        readabilityScore("COLEMAN–LIAU", analysis.stats.colemanLiauIndex)
                        readabilityScore("AUTOMATED", analysis.stats.automatedReadabilityIndex)
                    }
                    VStack(alignment: .leading, spacing: 10) {
                        readabilityScore("FLESCH–KINCAID", analysis.stats.fleschKincaidGrade)
                        readabilityScore("COLEMAN–LIAU", analysis.stats.colemanLiauIndex)
                        readabilityScore("AUTOMATED", analysis.stats.automatedReadabilityIndex)
                    }
                }
            }
        }
    }

    private func metricCard(_ label: String, _ value: String, _ symbol: String) -> some View {
        LabPanel {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(label)
                        .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                    Text(value)
                        .font(.system(size: Lab.size(24), weight: .black, design: .rounded))
                        .foregroundStyle(Lab.text)
                }
                Spacer()
                Image(systemName: symbol)
                    .font(.system(size: 24, weight: .bold))
                    .foregroundStyle(Lab.emerald)
            }
        }
    }

    private func readabilityScore(_ label: String, _ score: Double) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.system(size: Lab.size(8), weight: .black, design: .monospaced))
                .foregroundStyle(Lab.secondary)
            Text(String(format: "Grade %.1f", score))
                .font(.system(size: Lab.size(14), weight: .bold))
                .foregroundStyle(Lab.text)
        }
    }

    private func structurePanel(_ structure: DocumentStructureSummary) -> some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                LabLabel(text: "Structural anatomy")
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 125), spacing: 8)], spacing: 8) {
                    structureChip("Headings", structure.headingsTotal, "list.bullet.indent")
                    structureChip("Paragraphs", structure.paragraphs, "text.alignleft")
                    structureChip("Code", structure.codeBlocks, "chevron.left.forwardslash.chevron.right")
                    structureChip("Tables", structure.tables, "tablecells")
                    structureChip("Lists", structure.lists, "checklist")
                    structureChip("Images", structure.images, "photo")
                    structureChip("Links", structure.linksTotal, "link")
                    structureChip("Math", structure.mathBlocks + structure.mathInlines, "function")
                }
            }
        }
    }

    private func structureChip(_ title: String, _ value: Int, _ symbol: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: symbol).foregroundStyle(Lab.emerald)
            Text(title).lineLimit(1)
            Spacer()
            Text("\(value)").fontWeight(.black)
        }
        .font(.system(size: Lab.size(10)))
        .foregroundStyle(Lab.text)
        .padding(10)
        .background(Lab.panelSoft, in: RoundedRectangle(cornerRadius: 10))
    }

    private func findingsPanel(_ analysis: DocumentAnalysis) -> some View {
        let findings = analysis.stats.findings + analysis.audit.findings
        return LabPanel {
            VStack(alignment: .leading, spacing: 11) {
                HStack {
                    LabLabel(text: "Editorial health")
                    Spacer()
                    Text(findings.isEmpty ? "ALL CLEAR" : "\(findings.count) TO REVIEW")
                        .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                        .foregroundStyle(findings.isEmpty ? Lab.emerald : Lab.amber)
                }
                if findings.isEmpty {
                    Label("No structural or accessibility findings", systemImage: "checkmark.seal.fill")
                        .font(.system(size: Lab.size(13), weight: .bold))
                        .foregroundStyle(Lab.emerald)
                } else {
                    ForEach(findings) { finding in
                        HStack(alignment: .top, spacing: 9) {
                            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Lab.amber)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(finding.code.replacingOccurrences(of: "_", with: " ").uppercased())
                                    .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                                    .foregroundStyle(Lab.amber)
                                Text(finding.displayMessage)
                                    .font(.system(size: Lab.size(11)))
                                    .foregroundStyle(Lab.text)
                            }
                        }
                    }
                }
            }
        }
    }

    private func searchPanel(_ search: SearchIndexSummary) -> some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    LabLabel(text: "Search anatomy")
                    Spacer()
                    Text("\(search.entries.count) ANCHORED ENTRIES")
                        .font(.system(size: Lab.size(8), weight: .black, design: .monospaced))
                        .foregroundStyle(Lab.cyan)
                }
                ForEach(Array(search.entries.prefix(6).enumerated()), id: \.offset) { _, entry in
                    HStack(spacing: 9) {
                        Image(systemName: entry.kind == "heading" ? "number" : "text.alignleft")
                            .foregroundStyle(Lab.cyan)
                            .frame(width: 18)
                        Text(entry.text)
                            .font(.system(size: Lab.size(11), weight: .medium))
                            .foregroundStyle(Lab.text)
                            .lineLimit(1)
                        Spacer()
                        Text("#\(entry.anchor)")
                            .font(.system(size: Lab.size(8), design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
    }

    private func publishCard(_ title: String, _ detail: String, _ symbol: String, _ tint: Color, action: @escaping () async -> Void) -> some View {
        Button { Task { await action() } } label: {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Image(systemName: symbol)
                        .font(.system(size: 29, weight: .bold))
                        .foregroundStyle(tint)
                    Spacer()
                    Image(systemName: "arrow.up.forward.circle.fill")
                        .font(.system(size: 22, weight: .bold))
                        .foregroundStyle(tint.opacity(0.86))
                }
                Text(title)
                    .font(.system(size: Lab.size(18), weight: .black, design: .rounded))
                    .foregroundStyle(Lab.text)
                Text(detail)
                    .font(.system(size: Lab.size(11), weight: .medium))
                    .foregroundStyle(Lab.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(17)
            .frame(maxWidth: .infinity, minHeight: 150, alignment: .leading)
            .background(tint.opacity(0.065), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 18).stroke(tint.opacity(0.24)))
        }
        .buttonStyle(.plain)
        .disabled(isWorking)
    }

    private func diffMetrics(_ stats: SemanticDiffStats) -> some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 125), spacing: 10)], spacing: 10) {
            metricCard("SIMILAR", "\(Int((stats.similarityRatio * 100).rounded()))%", "circle.hexagongrid.fill")
            metricCard("ADDED", "+\(stats.wordsInserted)", "plus.circle.fill")
            metricCard("REMOVED", "−\(stats.wordsDeleted)", "minus.circle.fill")
            metricCard("MODIFIED", "\(stats.modifiedBlocks)", "pencil.circle.fill")
        }
    }

    private func loadingPanel(_ text: String) -> some View {
        LabPanel {
            HStack(spacing: 12) {
                ProgressView().tint(Lab.emerald)
                Text(text)
                    .font(.system(size: Lab.size(12), design: .monospaced))
                    .foregroundStyle(Lab.secondary)
            }
        }
    }

    private var processingBanner: some View {
        HStack(spacing: 12) {
            ProgressView().tint(Lab.onEmerald)
            VStack(alignment: .leading, spacing: 1) {
                Text("THE FORGE IS WORKING")
                    .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                Text(processingMessage)
                    .font(.system(size: Lab.size(11), weight: .bold))
            }
            Spacer()
            Image(systemName: "bolt.fill")
        }
        .foregroundStyle(Lab.onEmerald)
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(Lab.emerald, in: Capsule())
        .shadow(color: Lab.emerald.opacity(0.28), radius: 18, y: 8)
    }

    private var processingMessage: String {
        if case let .exporting(message) = renderer.phase { return message }
        return "Mapping source → structure → artifact"
    }

    private func errorBanner(_ message: String) -> some View {
        HStack(alignment: .top, spacing: 11) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Lab.amber)
            VStack(alignment: .leading, spacing: 2) {
                Text("THE FORGE NEEDS ATTENTION")
                    .font(.system(size: Lab.size(8), weight: .black, design: .monospaced))
                    .foregroundStyle(Lab.amber)
                Text(message)
                    .font(.system(size: Lab.size(11), weight: .medium))
                    .foregroundStyle(Lab.text)
            }
            Spacer()
            Button { errorMessage = nil } label: {
                Image(systemName: "xmark.circle.fill").foregroundStyle(Lab.secondary)
            }
            .buttonStyle(.plain)
        }
        .padding(13)
        .background(Lab.panelStrong, in: RoundedRectangle(cornerRadius: 15, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 15).stroke(Lab.amber.opacity(0.35)))
    }

    private func duration(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        return "\(seconds / 60)m \(seconds % 60)s"
    }

    private func wordCount(_ source: String) -> Int {
        source.split { $0.isWhitespace || $0.isNewline }.count
    }

    private func refreshAnalysis() async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            _ = try await renderer.analyzeDocument()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func runDiff() async {
        guard !baseline.isEmpty, !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            diffPreview = try await renderer.semanticDiff(from: baseline)
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func shareArtifact(_ format: DocumentArtifactFormat) async {
        await performArtifactExport(defaultExtension: format == .interactiveHTML ? "html" : format == .searchIndex ? "json" : format.rawValue) {
            try await renderer.exportArtifact(format)
        }
    }

    private func shareBook(_ format: BookArtifactFormat) async {
        await performArtifactExport(defaultExtension: format == .pdf ? "pdf" : "zip") {
            try await renderer.exportBook(files: bookFiles, format: format)
        }
    }

    private func shareLegacyPDF() async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            let (data, _, _) = try await renderer.exportPdf()
            try share(data: data, extension: "pdf")
        } catch { errorMessage = error.localizedDescription }
    }

    private func shareLegacyHTML() async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            let (html, _, _) = try await renderer.exportHtml()
            try share(data: Data(html.utf8), extension: "html")
        } catch { errorMessage = error.localizedDescription }
    }

    private func performArtifactExport(defaultExtension: String, operation: () async throws -> RenderedArtifact) async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            let artifact = try await operation()
            guard let data = artifact.data else { throw ImportError.invalidArtifact }
            try share(data: data, extension: artifact.extension.isEmpty ? defaultExtension : artifact.extension)
        } catch { errorMessage = error.localizedDescription }
    }

    private func share(data: Data, extension ext: String) throws {
        let rawName = renderer.documentTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseName = rawName.isEmpty ? "FrankenMarkdown-Document" : rawName
        let safeName = baseName.components(separatedBy: CharacterSet(charactersIn: "/:\\?%*|\"<>")).joined(separator: "-")
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(safeName).\(ext)")
        try data.write(to: url, options: .atomic)
        exportURL = url
        showShare = true
    }

    private func importBaseline(_ result: Result<[URL], Error>) {
        do {
            guard let url = try result.get().first else { return }
            baseline = try readSecurityScopedText(url)
            diffPreview = nil
            errorMessage = nil
        } catch { errorMessage = error.localizedDescription }
    }

    private func importBookFiles(_ result: Result<[URL], Error>) {
        do {
            let urls = try result.get()
            var imported: [BookSourceFile] = []
            for url in urls { imported.append(contentsOf: try markdownFiles(at: url)) }
            let unique = Dictionary(grouping: imported, by: \.path).compactMap { $0.value.first }
            bookFiles = unique.sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
            if bookFiles.isEmpty { throw ImportError.noMarkdownFiles }
            errorMessage = nil
        } catch { errorMessage = error.localizedDescription }
    }

    private func importStylesheet(_ result: Result<[URL], Error>) {
        do {
            guard let url = try result.get().first else { return }
            renderer.customCSS = try readSecurityScopedStyle(url)
            errorMessage = nil
        } catch { errorMessage = error.localizedDescription }
    }

    private func markdownFiles(at url: URL) throws -> [BookSourceFile] {
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        let values = try url.resourceValues(forKeys: [.isDirectoryKey])
        if values.isDirectory == true {
            let root = url.standardizedFileURL
            guard let enumerator = FileManager.default.enumerator(
                at: root,
                includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            ) else { throw ImportError.noMarkdownFiles }
            var files: [BookSourceFile] = []
            for case let child as URL in enumerator {
                guard ["md", "markdown"].contains(child.pathExtension.lowercased()) else { continue }
                let source = try String(contentsOf: child, encoding: .utf8)
                let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
                let relative = child.standardizedFileURL.path.replacingOccurrences(of: prefix, with: "")
                files.append(BookSourceFile(path: relative, source: source))
            }
            return files
        }
        guard ["md", "markdown"].contains(url.pathExtension.lowercased()) else { return [] }
        return [BookSourceFile(path: url.lastPathComponent, source: try String(contentsOf: url, encoding: .utf8))]
    }

    private func readSecurityScopedText(_ url: URL) throws -> String {
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        return try String(contentsOf: url, encoding: .utf8)
    }

    private func readSecurityScopedStyle(_ url: URL) throws -> String {
        guard url.isFileURL else { throw ImportError.invalidStylesheet }
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let maximum = MarkdownActiveDraft.maximumCustomCSSBytes
        let data = try handle.read(upToCount: maximum + 1) ?? Data()
        guard data.count <= maximum else { throw ImportError.stylesheetTooLarge }
        guard let stylesheet = String(data: data, encoding: .utf8) else {
            throw ImportError.invalidStylesheet
        }
        return stylesheet
    }

    private func presetSymbol(_ id: String) -> String {
        switch id {
        case "executive": "chart.bar.doc.horizontal.fill"
        case "research": "atom"
        case "product": "shippingbox.and.arrow.backward.fill"
        case "book": "books.vertical.fill"
        case "mermaid": "point.3.connected.trianglepath.dotted"
        default: "doc.text.image.fill"
        }
    }

    private func templateColor(_ id: String) -> Color {
        switch id {
        case "executive": Lab.amber
        case "research": Lab.cyan
        case "product": Color.orange
        case "book": Color.purple
        case "mermaid": Color.pink
        default: Lab.emerald
        }
    }
}

private enum ImportError: LocalizedError {
    case noMarkdownFiles
    case invalidArtifact
    case stylesheetTooLarge
    case invalidStylesheet

    var errorDescription: String? {
        switch self {
        case .noMarkdownFiles: "No .md or .markdown files were found in that selection."
        case .invalidArtifact: "The document engine returned an invalid artifact."
        case .stylesheetTooLarge: "That stylesheet is larger than the supported 256 KB limit."
        case .invalidStylesheet: "Choose a UTF-8 CSS text file."
        }
    }
}

private struct ReadabilitySpectrum: View {
    let score: Double

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(LinearGradient(colors: [Lab.danger, Lab.amber, Lab.emerald], startPoint: .leading, endPoint: .trailing))
                Circle()
                    .fill(.white)
                    .frame(width: 18, height: 18)
                    .shadow(color: .black.opacity(0.5), radius: 4)
                    .offset(x: max(0, min(proxy.size.width - 18, proxy.size.width * CGFloat(score / 100))))
            }
        }
        .frame(height: 18)
        .accessibilityLabel("Flesch reading ease \(Int(score.rounded())) out of 100")
    }
}

private struct ForgeCoreVisual: View {
    let active: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 24.0, paused: reduceMotion)) { timeline in
            Canvas { context, size in
                draw(
                    context: &context,
                    size: size,
                    time: timeline.date.timeIntervalSinceReferenceDate
                )
            }
        }
        .mask(LinearGradient(colors: [.clear, .white], startPoint: .leading, endPoint: .trailing))
        .accessibilityHidden(true)
    }

    private func draw(
        context: inout GraphicsContext,
        size: CGSize,
        time: TimeInterval
    ) {
        let center = CGPoint(x: size.width * 0.79, y: size.height * 0.5)
        let radius = min(size.width, size.height) * 0.23
        for ring in 0..<4 {
            let ringRadius = radius * (0.55 + CGFloat(ring) * 0.22)
            let rect = CGRect(
                x: center.x - ringRadius,
                y: center.y - ringRadius,
                width: ringRadius * 2,
                height: ringRadius * 2
            )
            let tint = ring.isMultiple(of: 2) ? Lab.emerald : Lab.cyan
            context.stroke(
                Path(ellipseIn: rect),
                with: .color(tint.opacity(0.08 + Double(ring) * 0.025)),
                lineWidth: 1
            )
        }
        for index in 0..<8 {
            let angle = (Double(index) / 8 * .pi * 2) + time * (active ? 0.32 : 0.08)
            let orbit = radius * (index.isMultiple(of: 2) ? 1.0 : 0.74)
            let point = CGPoint(
                x: center.x + CGFloat(cos(angle)) * orbit,
                y: center.y + CGFloat(sin(angle)) * orbit
            )
            let node = CGRect(x: point.x - 4, y: point.y - 4, width: 8, height: 8)
            let tint = index.isMultiple(of: 2) ? Lab.emerald : Lab.cyan
            context.fill(Path(ellipseIn: node), with: .color(tint.opacity(0.55)))
        }
        let pulse = 0.23 + 0.025 * CGFloat(sin(time * 1.7))
        let coreRadius = radius * pulse
        let core = CGRect(
            x: center.x - coreRadius,
            y: center.y - coreRadius,
            width: coreRadius * 2,
            height: coreRadius * 2
        )
        let gradient = Gradient(colors: [.white.opacity(0.8), Lab.emerald.opacity(0.48), .clear])
        context.fill(
            Path(ellipseIn: core),
            with: .radialGradient(
                gradient,
                center: center,
                startRadius: 0,
                endRadius: coreRadius * 1.8
            )
        )
    }
}

private struct OutputConstellationAnimation: View {
    let active: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 24.0, paused: reduceMotion || !active)) { timeline in
            Canvas { context, size in
                draw(
                    context: &context,
                    size: size,
                    time: timeline.date.timeIntervalSinceReferenceDate
                )
            }
        }
        .accessibilityHidden(true)
    }

    private func draw(
        context: inout GraphicsContext,
        size: CGSize,
        time: TimeInterval
    ) {
        let center = CGPoint(x: size.width / 2, y: size.height / 2)
        let colors: [Color] = [Lab.amber, Lab.cyan, Lab.emerald, .purple, .orange, .blue]
        for index in 0..<6 {
            let angle = Double(index) / 6 * .pi * 2 - .pi / 2
            let point = CGPoint(
                x: center.x + CGFloat(cos(angle)) * size.width * 0.36,
                y: center.y + CGFloat(sin(angle)) * size.height * 0.34
            )
            var path = Path()
            path.move(to: center)
            path.addLine(to: point)
            context.stroke(path, with: .color(colors[index].opacity(0.22)), lineWidth: 1.5)
            let wave = active ? (sin(time * 3 + Double(index)) + 1) / 2 : 0.35
            let radius = CGFloat(7 + wave * 4)
            let node = CGRect(
                x: point.x - radius,
                y: point.y - radius,
                width: radius * 2,
                height: radius * 2
            )
            context.fill(
                Path(ellipseIn: node),
                with: .color(colors[index].opacity(0.45 + wave * 0.4))
            )
        }
        let core = CGRect(x: center.x - 14, y: center.y - 14, width: 28, height: 28)
        context.fill(Path(ellipseIn: core), with: .color(Lab.emerald.opacity(0.8)))
    }
}

private struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        layout(proposal: proposal, subviews: subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = layout(proposal: ProposedViewSize(width: bounds.width, height: proposal.height), subviews: subviews)
        for (index, point) in result.points.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + point.x, y: bounds.minY + point.y), proposal: .unspecified)
        }
    }

    private func layout(proposal: ProposedViewSize, subviews: Subviews) -> (size: CGSize, points: [CGPoint]) {
        let width = proposal.width ?? 600
        var points: [CGPoint] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var lineHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += lineHeight + spacing
                lineHeight = 0
            }
            points.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }
        return (CGSize(width: width, height: y + lineHeight), points)
    }
}

private struct DiffWebView: UIViewRepresentable {
    let html: String

    func makeUIView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.isOpaque = false
        view.backgroundColor = .clear
        view.loadHTMLString(html, baseURL: nil)
        return view
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {
        guard context.coordinator.lastHTML != html else { return }
        context.coordinator.lastHTML = html
        uiView.loadHTMLString(html, baseURL: nil)
    }

    func makeCoordinator() -> Coordinator { Coordinator(html: html) }

    final class Coordinator {
        var lastHTML: String
        init(html: String) { lastHTML = html }
    }
}
