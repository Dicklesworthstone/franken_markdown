import SwiftUI
import UniformTypeIdentifiers
import WebKit

private enum DocumentLabSection: String, CaseIterable, Identifiable {
    case intelligence = "Intelligence"
    case export = "Publish"
    case compare = "Compare"
    case book = "Book"
    case templates = "Templates"

    var id: Self { self }
    var symbol: String {
        switch self {
        case .intelligence: "waveform.path.ecg.rectangle"
        case .export: "sparkles.rectangle.stack"
        case .compare: "arrow.left.arrow.right.square"
        case .book: "books.vertical.fill"
        case .templates: "rectangle.stack.badge.plus"
        }
    }
}

struct DocumentLabView: View {
    @ObservedObject var renderer: MarkdownRendererModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass

    @State private var section: DocumentLabSection = .intelligence
    @State private var isWorking = false
    @State private var errorMessage: String?
    @State private var exportURL: URL?
    @State private var showShare = false
    @State private var baseline = ""
    @State private var diffPreview: SemanticDiffPreview?
    @State private var showBaselineImporter = false
    @State private var showBookImporter = false
    @State private var bookFiles: [BookSourceFile] = []

    var body: some View {
        NavigationStack {
            GeometryReader { geometry in
                ZStack {
                    LaboratoryBackground()
                    if geometry.size.width >= 820 {
                        HStack(spacing: 0) {
                            sidebar
                                .frame(width: 230)
                            Divider().overlay(Lab.stroke)
                            content
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                        }
                    } else {
                        VStack(spacing: 12) {
                            compactSectionPicker
                            content
                        }
                        .padding(.horizontal, 14)
                    }
                }
            }
            .navigationTitle("Document Lab")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $showShare) {
            if let exportURL { ShareActivityView(activityItems: [exportURL]) }
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
        .alert("The laboratory hit a snag", isPresented: Binding(
            get: { errorMessage != nil },
            set: { if !$0 { errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "Unknown error")
        }
        .task {
            if renderer.analysisIsStale { await refreshAnalysis() }
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 10) {
                Image(systemName: "bolt.horizontal.circle.fill")
                    .font(.system(size: 28, weight: .black))
                    .foregroundStyle(Lab.emerald)
                VStack(alignment: .leading, spacing: 1) {
                    Text("DOCUMENT LAB")
                        .font(.system(size: Lab.size(13), weight: .black, design: .monospaced))
                        .foregroundStyle(Lab.text)
                    Text("one source · many forms")
                        .font(.system(size: Lab.size(9), design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                }
            }
            .padding(.bottom, 8)

            ForEach(DocumentLabSection.allCases) { candidate in
                Button {
                    withAnimation(.snappy) { section = candidate }
                } label: {
                    Label(candidate.rawValue, systemImage: candidate.symbol)
                        .font(.system(size: Lab.size(12), weight: .bold))
                        .foregroundStyle(section == candidate ? Color.black : Lab.text)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 12)
                        .frame(height: 44)
                        .background(
                            section == candidate ? Lab.emerald : Color.white.opacity(0.025),
                            in: RoundedRectangle(cornerRadius: 12, style: .continuous)
                        )
                }
                .buttonStyle(.plain)
            }

            Spacer()
            Label("Pure Rust · offline", systemImage: "network.slash")
                .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                .foregroundStyle(Lab.secondary)
        }
        .padding(18)
        .background(Color.black.opacity(0.2))
    }

    private var compactSectionPicker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(DocumentLabSection.allCases) { candidate in
                    Button {
                        withAnimation(.snappy) { section = candidate }
                    } label: {
                        Label(candidate.rawValue, systemImage: candidate.symbol)
                            .font(.system(size: Lab.size(10), weight: .bold))
                            .foregroundStyle(section == candidate ? Color.black : Lab.text)
                            .padding(.horizontal, 12)
                            .frame(height: 38)
                            .background(
                                section == candidate ? Lab.emerald : Color.black.opacity(0.38),
                                in: Capsule()
                            )
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, 2)
        }
    }

    @ViewBuilder
    private var content: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                switch section {
                case .intelligence: intelligenceView
                case .export: publishView
                case .compare: compareView
                case .book: bookView
                case .templates: templatesView
                }
            }
            .frame(maxWidth: 1_050, alignment: .leading)
            .padding(horizontalSizeClass == .regular ? 24 : 2)
            .padding(.vertical, 18)
        }
        .scrollIndicators(.hidden)
    }

    private var intelligenceView: some View {
        VStack(alignment: .leading, spacing: 16) {
            processHero
            if let analysis = renderer.analysis {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 145), spacing: 12)], spacing: 12) {
                    metricCard("WORDS", "\(analysis.stats.words)", "text.word.spacing")
                    metricCard("READ", duration(analysis.stats.readingTimeSeconds), "book.pages")
                    metricCard("SPEAK", duration(analysis.stats.speakingTimeSeconds), "waveform")
                    metricCard("EASE", String(format: "%.0f", analysis.stats.fleschReadingEase), "gauge.with.dots.needle.50percent")
                }

                LabPanel {
                    VStack(alignment: .leading, spacing: 14) {
                        HStack {
                            LabLabel(text: "Readability spectrum")
                            Spacer()
                            Text(analysis.stats.readingEaseLabel)
                                .font(.system(size: Lab.size(10), weight: .bold))
                                .foregroundStyle(Lab.emerald)
                        }
                        ReadabilitySpectrum(score: analysis.stats.fleschReadingEase)
                        HStack(spacing: 18) {
                            readabilityScore("F–K", analysis.stats.fleschKincaidGrade)
                            readabilityScore("C–L", analysis.stats.colemanLiauIndex)
                            readabilityScore("ARI", analysis.stats.automatedReadabilityIndex)
                        }
                    }
                }

                structurePanel(analysis.stats.structure)
                findingsPanel(analysis)
            } else {
                loadingPanel("The Rust engine is mapping prose, structure, accessibility, and anchors…")
            }
        }
    }

    private var processHero: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        LabLabel(text: "Document nervous system")
                        Text("SOURCE → AST → SEMANTICS → LAYOUT → ARTIFACT")
                            .font(.system(size: Lab.size(9), weight: .bold, design: .monospaced))
                            .foregroundStyle(Lab.secondary)
                    }
                    Spacer()
                    Button {
                        Task { await refreshAnalysis() }
                    } label: {
                        Label(renderer.analysisIsStale ? "Refresh" : "Recheck", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(.bordered)
                    .tint(Lab.emerald)
                    .disabled(isWorking)
                }
                DocumentCircuitAnimation(active: isWorking)
                    .frame(height: 92)
            }
        }
    }

    private var publishView: some View {
        VStack(alignment: .leading, spacing: 16) {
            sectionIntro(
                "Publish in every form",
                "One parsed document becomes polished reading, print, vector, e-book, a portable living workspace, or a deterministic search corpus."
            )
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
                publishCard("PDF", "Print-perfect, tagged and selectable", "doc.richtext", Lab.amber) {
                    await shareLegacyPDF()
                }
                publishCard("HTML", "Self-contained reading view", "globe", Lab.cyan) {
                    await shareLegacyHTML()
                }
                publishCard("Vector Poster", "Every glyph converted to SVG paths", "scribble.variable", Lab.emerald) {
                    await shareArtifact(.svg)
                }
                publishCard("EPUB 3", "A standards-native offline e-book", "books.vertical", Color.purple) {
                    await shareArtifact(.epub)
                }
                publishCard("Living Workspace", "Editor, preview, stats and print in one HTML file", "sparkles.rectangle.stack", Color.orange) {
                    await shareArtifact(.interactiveHTML)
                }
                publishCard("Search Index", "Deterministic anchored JSON for instant search", "magnifyingglass.circle", Color.blue) {
                    await shareArtifact(.searchIndex)
                }
            }
            exportSettings
        }
    }

    private var compareView: some View {
        VStack(alignment: .leading, spacing: 16) {
            sectionIntro(
                "Semantic time machine",
                "Compare Markdown meaning—not noisy lines. The engine aligns blocks, then reveals word-level changes inside modified passages."
            )
            LabPanel {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(baseline.isEmpty ? "Choose a baseline" : "Baseline armed")
                                .font(.system(size: Lab.size(16), weight: .black))
                                .foregroundStyle(Lab.text)
                            Text(baseline.isEmpty ? "Import an earlier .md file or snapshot the current source." : "\(baseline.utf8.count) bytes ready for structural alignment")
                                .font(.system(size: Lab.size(11)))
                                .foregroundStyle(Lab.secondary)
                        }
                        Spacer()
                        Image(systemName: baseline.isEmpty ? "clock.badge.questionmark" : "clock.badge.checkmark.fill")
                            .font(.system(size: 32, weight: .bold))
                            .foregroundStyle(baseline.isEmpty ? Lab.amber : Lab.emerald)
                    }
                    HStack {
                        Button("Import earlier version") { showBaselineImporter = true }
                            .buttonStyle(.borderedProminent)
                            .tint(Lab.emerald)
                        Button("Snapshot current") { baseline = renderer.source; diffPreview = nil }
                            .buttonStyle(.bordered)
                        Spacer()
                        Button("Compare") { Task { await runDiff() } }
                            .buttonStyle(.borderedProminent)
                            .tint(Lab.amber)
                            .disabled(baseline.isEmpty || isWorking)
                    }
                }
            }

            if let diffPreview {
                diffMetrics(diffPreview.metrics.stats)
                LabPanel {
                    DiffWebView(html: diffPreview.html)
                        .frame(minHeight: 460)
                        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
            }
        }
    }

    private var bookView: some View {
        VStack(alignment: .leading, spacing: 16) {
            sectionIntro(
                "Bind a complete book",
                "Select a Markdown folder. FrankenMarkdown expands includes, rewrites chapter links, creates navigation and search, and binds one continuous PDF—all locally."
            )
            LabPanel {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Label("\(bookFiles.count) chapter\(bookFiles.count == 1 ? "" : "s")", systemImage: "books.vertical.fill")
                            .font(.system(size: Lab.size(15), weight: .black))
                            .foregroundStyle(bookFiles.isEmpty ? Lab.secondary : Lab.text)
                        Spacer()
                        Button(bookFiles.isEmpty ? "Choose folder" : "Replace files") {
                            showBookImporter = true
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(Lab.emerald)
                    }
                    if bookFiles.isEmpty {
                        Text("Choose a folder or a group of .md files. Relative paths and {{#include …}} directives are preserved inside the selected group.")
                            .font(.system(size: Lab.size(12)))
                            .foregroundStyle(Lab.secondary)
                    } else {
                        VStack(spacing: 0) {
                            ForEach(bookFiles) { file in
                                HStack(spacing: 10) {
                                    Image(systemName: "doc.text")
                                        .foregroundStyle(Lab.emerald)
                                    Text(file.path)
                                        .font(.system(size: Lab.size(11), design: .monospaced))
                                        .foregroundStyle(Lab.text)
                                        .lineLimit(1)
                                    Spacer()
                                    Text("\(file.source.utf8.count) B")
                                        .font(.system(size: Lab.size(9), design: .monospaced))
                                        .foregroundStyle(Lab.secondary)
                                }
                                .padding(.vertical, 8)
                                Divider().overlay(Lab.stroke)
                            }
                        }
                    }
                    HStack {
                        Button { Task { await shareBook(.site) } } label: {
                            Label("Build site ZIP", systemImage: "shippingbox.fill")
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(Lab.cyan)
                        Button { Task { await shareBook(.pdf) } } label: {
                            Label("Bind PDF book", systemImage: "book.closed.fill")
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(Lab.amber)
                    }
                    .disabled(bookFiles.isEmpty || isWorking)
                }
            }
        }
    }

    private var templatesView: some View {
        VStack(alignment: .leading, spacing: 16) {
            sectionIntro(
                "Start from something extraordinary",
                "These are full editorial systems, not toy snippets: polished structure, tables, mathematics, diagrams, callouts, citations, and accessibility-aware authoring."
            )
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 245), spacing: 12)], spacing: 12) {
                ForEach(MarkdownRendererModel.presets) { preset in
                    Button {
                        renderer.source = preset.markdown
                        renderer.documentTitle = preset.title
                        dismiss()
                    } label: {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack {
                                Image(systemName: presetSymbol(preset.id))
                                    .font(.system(size: 25, weight: .bold))
                                    .foregroundStyle(templateColor(preset.id))
                                Spacer()
                                Text("\(preset.markdown.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }.count) words")
                                    .font(.system(size: Lab.size(9), design: .monospaced))
                                    .foregroundStyle(Lab.secondary)
                            }
                            Text(preset.title)
                                .font(.system(size: Lab.size(16), weight: .black))
                                .foregroundStyle(Lab.text)
                            Text(preset.description)
                                .font(.system(size: Lab.size(11)))
                                .foregroundStyle(Lab.secondary)
                                .multilineTextAlignment(.leading)
                                .lineLimit(3)
                            Label("Use this system", systemImage: "arrow.right.circle.fill")
                                .font(.system(size: Lab.size(10), weight: .bold, design: .monospaced))
                                .foregroundStyle(Lab.emerald)
                        }
                        .padding(15)
                        .frame(maxWidth: .infinity, minHeight: 175, alignment: .leading)
                        .background(Color.black.opacity(0.42), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 16).stroke(templateColor(preset.id).opacity(0.3)))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var exportSettings: some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                LabLabel(text: "Editorial controls")
                TextField("Document title", text: $renderer.documentTitle)
                    .textFieldStyle(.roundedBorder)
                TextField("Author", text: $renderer.documentAuthor)
                    .textFieldStyle(.roundedBorder)
                HStack {
                    Picker("Language", selection: $renderer.language) {
                        Text("English").tag("en")
                        Text("German").tag("de")
                        Text("French").tag("fr")
                        Text("Spanish").tag("es")
                        Text("Dutch").tag("nl")
                    }
                    Toggle("Contents", isOn: $renderer.toc)
                    Toggle("Page numbers", isOn: $renderer.pageNumbers)
                }
                .font(.system(size: Lab.size(11)))
                Toggle("Optical-margin microtype for PDF", isOn: $renderer.microtypeProtrusion)
                    .font(.system(size: Lab.size(11)))
            }
        }
    }

    private func sectionIntro(_ title: String, _ subtitle: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.system(size: Lab.size(25), weight: .black, design: .rounded))
                .foregroundStyle(Lab.text)
            Text(subtitle)
                .font(.system(size: Lab.size(13)))
                .foregroundStyle(Lab.secondary)
                .fixedSize(horizontal: false, vertical: true)
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
                .font(.system(size: Lab.size(9), weight: .black, design: .monospaced))
                .foregroundStyle(Lab.secondary)
            Text(String(format: "Grade %.1f", score))
                .font(.system(size: Lab.size(13), weight: .bold))
                .foregroundStyle(Lab.text)
        }
    }

    private func structurePanel(_ structure: DocumentStructureSummary) -> some View {
        LabPanel {
            VStack(alignment: .leading, spacing: 12) {
                LabLabel(text: "Structural anatomy")
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 115), spacing: 8)], spacing: 8) {
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
        .padding(9)
        .background(Color.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 9))
    }

    private func findingsPanel(_ analysis: DocumentAnalysis) -> some View {
        let findings = analysis.stats.findings + analysis.audit.findings
        return LabPanel {
            VStack(alignment: .leading, spacing: 10) {
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
                            Image(systemName: "exclamationmark.triangle.fill")
                                .foregroundStyle(Lab.amber)
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

    private func publishCard(
        _ title: String,
        _ detail: String,
        _ symbol: String,
        _ tint: Color,
        action: @escaping () async -> Void
    ) -> some View {
        Button {
            Task { await action() }
        } label: {
            HStack(spacing: 13) {
                Image(systemName: symbol)
                    .font(.system(size: 27, weight: .bold))
                    .foregroundStyle(tint)
                    .frame(width: 44)
                VStack(alignment: .leading, spacing: 3) {
                    Text(title)
                        .font(.system(size: Lab.size(15), weight: .black))
                        .foregroundStyle(Lab.text)
                    Text(detail)
                        .font(.system(size: Lab.size(10)))
                        .foregroundStyle(Lab.secondary)
                        .multilineTextAlignment(.leading)
                        .lineLimit(2)
                }
                Spacer()
                Image(systemName: "arrow.up.forward.circle.fill")
                    .foregroundStyle(tint.opacity(0.8))
            }
            .padding(14)
            .frame(maxWidth: .infinity, minHeight: 86, alignment: .leading)
            .background(Color.black.opacity(0.42), in: RoundedRectangle(cornerRadius: 15, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 15).stroke(tint.opacity(0.28)))
        }
        .buttonStyle(.plain)
        .disabled(isWorking)
    }

    private func diffMetrics(_ stats: SemanticDiffStats) -> some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 10)], spacing: 10) {
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

    private func duration(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        return "\(seconds / 60)m \(seconds % 60)s"
    }

    private func refreshAnalysis() async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do { _ = try await renderer.analyzeDocument() }
        catch { errorMessage = error.localizedDescription }
    }

    private func runDiff() async {
        guard !baseline.isEmpty, !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        do { diffPreview = try await renderer.semanticDiff(from: baseline) }
        catch { errorMessage = error.localizedDescription }
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

    private func performArtifactExport(
        defaultExtension: String,
        operation: () async throws -> RenderedArtifact
    ) async {
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
        } catch { errorMessage = error.localizedDescription }
    }

    private func importBookFiles(_ result: Result<[URL], Error>) {
        do {
            let urls = try result.get()
            var imported: [BookSourceFile] = []
            for url in urls {
                imported.append(contentsOf: try markdownFiles(at: url))
            }
            let unique = Dictionary(grouping: imported, by: \.path).compactMap { _, values in values.first }
            bookFiles = unique.sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
            if bookFiles.isEmpty { throw ImportError.noMarkdownFiles }
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
        guard ["md", "markdown"].contains(url.pathExtension.lowercased()) else {
            return []
        }
        return [BookSourceFile(path: url.lastPathComponent, source: try String(contentsOf: url, encoding: .utf8))]
    }

    private func readSecurityScopedText(_ url: URL) throws -> String {
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        return try String(contentsOf: url, encoding: .utf8)
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

    var errorDescription: String? {
        switch self {
        case .noMarkdownFiles: "No .md or .markdown files were found in that selection."
        case .invalidArtifact: "The document engine returned an invalid artifact."
        }
    }
}

private struct ReadabilitySpectrum: View {
    let score: Double

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(
                    LinearGradient(
                        colors: [Lab.danger, Lab.amber, Lab.emerald],
                        startPoint: .leading,
                        endPoint: .trailing
                    )
                )
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

private struct DocumentCircuitAnimation: View {
    let active: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: reduceMotion || !active)) { timeline in
            Canvas { context, size in
                let nodes = 5
                let y = size.height / 2
                let step = size.width / CGFloat(nodes - 1)
                let phase = timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.8) / 1.8
                var wire = Path()
                wire.move(to: CGPoint(x: 10, y: y))
                wire.addLine(to: CGPoint(x: size.width - 10, y: y))
                context.stroke(wire, with: .color(Lab.emerald.opacity(0.26)), lineWidth: 2)

                for index in 0..<nodes {
                    let x = CGFloat(index) * step
                    let pulse = active && !reduceMotion
                        ? max(0, 1 - abs(Double(index) / Double(nodes - 1) - phase) * 6)
                        : (index == 0 ? 0.5 : 0.15)
                    context.fill(
                        Path(ellipseIn: CGRect(x: x - 12, y: y - 12, width: 24, height: 24)),
                        with: .color(Lab.emerald.opacity(0.24 + pulse * 0.7))
                    )
                    context.stroke(
                        Path(ellipseIn: CGRect(x: x - 12, y: y - 12, width: 24, height: 24)),
                        with: .color(Lab.emerald.opacity(0.8)),
                        lineWidth: 1.2
                    )
                }
                if active && !reduceMotion {
                    let x = CGFloat(phase) * size.width
                    context.fill(
                        Path(ellipseIn: CGRect(x: x - 6, y: y - 6, width: 12, height: 12)),
                        with: .color(.white)
                    )
                }
            }
        }
        .overlay(alignment: .bottom) {
            HStack {
                ForEach(["SOURCE", "AST", "SEMANTICS", "LAYOUT", "OUTPUT"], id: \.self) { label in
                    Text(label)
                        .font(.system(size: Lab.size(7), weight: .black, design: .monospaced))
                        .foregroundStyle(Lab.secondary)
                    if label != "OUTPUT" { Spacer() }
                }
            }
        }
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
