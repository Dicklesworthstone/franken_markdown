import Foundation
import SwiftUI
import WebKit

enum RenderPhase: Equatable {
    case loading
    case ready
    case rendering
    case exporting(String)
    case failed(String)
}

struct DocumentHeading: Identifiable, Hashable {
    let id = UUID()
    let level: Int
    let title: String
    let lineNumber: Int
}

struct DocumentPreset: Identifiable {
    let id: String
    let title: String
    let description: String
    let markdown: String
}

struct DocumentFinding: Decodable, Identifiable, Hashable {
    let severity: String?
    let code: String
    let message: String?
    let detail: String?
    var id: String { "\(code):\(message ?? detail ?? "")" }
    var displayMessage: String { message ?? detail ?? code }
}

struct DocumentStructureSummary: Decodable, Hashable {
    let headingsTotal: Int
    let paragraphs: Int
    let codeBlocks: Int
    let tables: Int
    let lists: Int
    let images: Int
    let linksTotal: Int
    let mathBlocks: Int
    let mathInlines: Int

    enum CodingKeys: String, CodingKey {
        case headingsTotal = "headings_total"
        case paragraphs
        case codeBlocks = "code_blocks"
        case tables, lists, images
        case linksTotal = "links_total"
        case mathBlocks = "math_blocks"
        case mathInlines = "math_inlines"
    }
}

struct DocumentStatsSummary: Decodable, Hashable {
    let bytes: Int
    let lines: Int
    let words: Int
    let characters: Int
    let sentences: Int
    let readingTimeSeconds: Int
    let speakingTimeSeconds: Int
    let fleschReadingEase: Double
    let fleschKincaidGrade: Double
    let colemanLiauIndex: Double
    let automatedReadabilityIndex: Double
    let readingEaseLabel: String
    let structure: DocumentStructureSummary
    let findings: [DocumentFinding]

    enum CodingKeys: String, CodingKey {
        case bytes, lines, words, characters, sentences, structure, findings
        case readingTimeSeconds = "reading_time_secs"
        case speakingTimeSeconds = "speaking_time_secs"
        case fleschReadingEase = "flesch_reading_ease"
        case fleschKincaidGrade = "flesch_kincaid_grade"
        case colemanLiauIndex = "coleman_liau_index"
        case automatedReadabilityIndex = "automated_readability_index"
        case readingEaseLabel = "reading_ease_label"
    }
}

struct AccessibilityAuditSummary: Decodable, Hashable {
    let verdict: String
    let findings: [DocumentFinding]
}

struct SearchIndexSummary: Decodable, Hashable {
    struct Entry: Decodable, Hashable {
        let kind: String
        let level: Int?
        let anchor: String
        let text: String
    }
    let entries: [Entry]
}

struct DocumentAnalysis: Decodable, Hashable {
    let stats: DocumentStatsSummary
    let audit: AccessibilityAuditSummary
    let search: SearchIndexSummary
}

struct SemanticDiffStats: Decodable, Hashable {
    let unchangedBlocks: Int
    let insertedBlocks: Int
    let deletedBlocks: Int
    let modifiedBlocks: Int
    let wordsInserted: Int
    let wordsDeleted: Int
    let similarityRatio: Double

    enum CodingKeys: String, CodingKey {
        case unchangedBlocks = "unchanged_blocks"
        case insertedBlocks = "inserted_blocks"
        case deletedBlocks = "deleted_blocks"
        case modifiedBlocks = "modified_blocks"
        case wordsInserted = "words_inserted"
        case wordsDeleted = "words_deleted"
        case similarityRatio = "similarity_ratio"
    }
}

struct SemanticDiffReport: Decodable, Hashable {
    let oldName: String
    let newName: String
    let stats: SemanticDiffStats

    enum CodingKeys: String, CodingKey {
        case oldName = "old_name"
        case newName = "new_name"
        case stats
    }
}

struct SemanticDiffPreview: Decodable, Hashable {
    let html: String
    let metrics: SemanticDiffReport
}

struct BookSourceFile: Identifiable, Hashable {
    let path: String
    let source: String
    var id: String { path }
}

enum DocumentArtifactFormat: String, CaseIterable, Identifiable {
    case svg
    case epub
    case interactiveHTML = "interactive-html"
    case searchIndex = "search-index"
    var id: Self { self }

    var title: String {
        switch self {
        case .svg: "Vector Poster"
        case .epub: "EPUB 3"
        case .interactiveHTML: "Living Workspace"
        case .searchIndex: "Search Index"
        }
    }
}

enum BookArtifactFormat: String {
    case site
    case pdf
}

struct RenderedArtifact: Decodable {
    let base64: String
    let byteLength: Int
    let `extension`: String
    let mimeType: String

    enum CodingKeys: String, CodingKey {
        case base64, byteLength, `extension`, mimeType
    }

    var data: Data? { Data(base64Encoded: base64) }
}

@MainActor
final class MarkdownRendererModel: NSObject, ObservableObject {
    @Published var source = MarkdownRendererModel.sample
    @Published var fontFamily = "sans"
    @Published var darkMode = "auto"
    @Published var allowRawHtml = false
    @Published var toc = false
    @Published var tocDepth = 3
    @Published var pageNumbers = false
    @Published var codeLineNumbers = false
    @Published var documentTitle = ""
    @Published var documentAuthor = ""
    @Published var language = "en"
    @Published var microtypeProtrusion = false
    @Published var fitToPages = 0
    @Published private(set) var analysis: DocumentAnalysis?
    @Published private(set) var analysisIsStale = true
    var renderFontScale = 1.0

    @Published private(set) var phase: RenderPhase = .loading
    @Published private(set) var elapsedMS: Double?
    @Published private(set) var outputBytes = 0
    @Published private(set) var diagnosticCount = 0

    let webView: WKWebView
    private var requestID = 0
    private var scheduledRender: Task<Void, Never>?

    private var pdfContinuations: [Int: CheckedContinuation<(Data, Int, Int), Error>] = [:]
    private var htmlContinuations: [Int: CheckedContinuation<(String, Int, Int), Error>] = [:]

    /// The Rust/WASM ABI intentionally supports only adaptive dark CSS or a
    /// light-only document. Keep the bridge fail-safe even if a future UI or
    /// restored state supplies an unsupported value.
    private var validatedDarkMode: String {
        darkMode == "disabled" ? "disabled" : "auto"
    }

    override init() {
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(FrankenResourceSchemeHandler(), forURLScheme: "frankenmd")
        configuration.defaultWebpagePreferences.allowsContentJavaScript = true
        configuration.websiteDataStore = .nonPersistent()
        webView = WKWebView(frame: .zero, configuration: configuration)
        super.init()
        configuration.userContentController.add(self, name: "frankenBridge")
        webView.navigationDelegate = self
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        webView.load(URLRequest(url: URL(string: "frankenmd://bundle/bridge.html")!))
    }

    deinit {
        scheduledRender?.cancel()
    }

    var headings: [DocumentHeading] {
        var list: [DocumentHeading] = []
        let lines = source.components(separatedBy: .newlines)
        for (idx, line) in lines.enumerated() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("#") {
                let hashes = trimmed.prefix(while: { $0 == "#" })
                let level = hashes.count
                if level >= 1 && level <= 6 && trimmed.dropFirst(level).starts(with: " ") {
                    let title = trimmed.dropFirst(level).trimmingCharacters(in: .whitespaces)
                    list.append(DocumentHeading(level: level, title: title, lineNumber: idx + 1))
                }
            }
        }
        return list
    }

    private var sharedOptions: [String: Any] {
        var options: [String: Any] = [
            "darkMode": validatedDarkMode,
            "allowRawHtml": allowRawHtml,
            "font": fontFamily,
            "fontScale": renderFontScale,
            "lang": language,
            "toc": toc,
            "tocDepth": tocDepth
        ]
        if !documentTitle.isEmpty { options["title"] = documentTitle }
        if !documentAuthor.isEmpty { options["author"] = documentAuthor }
        return options
    }

    func scheduleRender() {
        scheduledRender?.cancel()
        analysisIsStale = true
        let expectedSource = source
        scheduledRender = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(180))
            guard !Task.isCancelled, let self, self.source == expectedSource else { return }
            self.renderNow()
        }
    }

    func renderNow() {
        guard phase != .loading, !source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return
        }
        requestID += 1
        let options = sharedOptions
        let command: [String: Any] = [
            "requestID": requestID,
            "markdown": source,
            "options": options
        ]
        phase = .rendering
        Task { [weak self, weak webView] in
            do {
                _ = try await webView?.callAsyncJavaScript(
                    "return await window.frankenRender(command)",
                    arguments: ["command": command],
                    in: nil,
                    contentWorld: .page
                )
            } catch {
                self?.phase = .failed(error.localizedDescription)
            }
        }
    }

    func exportPdf() async throws -> (Data, Int, Int) {
        requestID += 1
        let req = requestID
        var options = sharedOptions
        options["pageNumbers"] = pageNumbers
        options["codeLineNumbers"] = codeLineNumbers
        options["microtype"] = microtypeProtrusion ? "protrusion" : "disabled"
        if fitToPages > 0 { options["fitToPages"] = fitToPages }
        let command: [String: Any] = [
            "requestID": req,
            "markdown": source,
            "options": options
        ]
        phase = .exporting("Forging PDF...")
        return try await withCheckedThrowingContinuation { continuation in
            self.pdfContinuations[req] = continuation
            Task { [weak self, weak webView] in
                do {
                    _ = try await webView?.callAsyncJavaScript(
                        "return await window.frankenExportPdf(command)",
                        arguments: ["command": command],
                        in: nil,
                        contentWorld: .page
                    )
                } catch {
                    self?.pdfContinuations.removeValue(forKey: req)?.resume(throwing: error)
                    self?.phase = .ready
                }
            }
        }
    }

    func exportHtml() async throws -> (String, Int, Int) {
        requestID += 1
        let req = requestID
        let options = sharedOptions
        let command: [String: Any] = [
            "requestID": req,
            "markdown": source,
            "options": options
        ]
        phase = .exporting("Forging HTML...")
        return try await withCheckedThrowingContinuation { continuation in
            self.htmlContinuations[req] = continuation
            Task { [weak self, weak webView] in
                do {
                    _ = try await webView?.callAsyncJavaScript(
                        "return await window.frankenExportHtml(command)",
                        arguments: ["command": command],
                        in: nil,
                        contentWorld: .page
                    )
                } catch {
                    self?.htmlContinuations.removeValue(forKey: req)?.resume(throwing: error)
                    self?.phase = .ready
                }
            }
        }
    }

    func analyzeDocument() async throws -> DocumentAnalysis {
        try await waitForBridgeReady()
        phase = .exporting("Inspecting structure...")
        defer { phase = .ready }
        let command: [String: Any] = ["markdown": source]
        let result: DocumentAnalysis = try await callBridgeJSON(
            function: "return await window.frankenAnalyze(command)",
            arguments: ["command": command]
        )
        analysis = result
        analysisIsStale = false
        return result
    }

    func exportArtifact(_ format: DocumentArtifactFormat) async throws -> RenderedArtifact {
        try await waitForBridgeReady()
        phase = .exporting("Forging \(format.title)...")
        defer { phase = .ready }
        let command: [String: Any] = [
            "format": format.rawValue,
            "markdown": source,
            "options": sharedOptions
        ]
        return try await callBridgeJSON(
            function: "return await window.frankenExportArtifact(command)",
            arguments: ["command": command]
        )
    }

    func semanticDiff(from baseline: String) async throws -> SemanticDiffPreview {
        try await waitForBridgeReady()
        phase = .exporting("Aligning semantic structure...")
        defer { phase = .ready }
        let command: [String: Any] = [
            "oldMarkdown": baseline,
            "newMarkdown": source,
            "options": ["oldName": "Before", "newName": "Current"]
        ]
        return try await callBridgeJSON(
            function: "return await window.frankenDiff(command)",
            arguments: ["command": command]
        )
    }

    func exportBook(
        files: [BookSourceFile],
        format: BookArtifactFormat
    ) async throws -> RenderedArtifact {
        try await waitForBridgeReady()
        phase = .exporting(format == .pdf ? "Binding PDF book..." : "Building book site...")
        defer { phase = .ready }
        let filePayload = files.map { ["path": $0.path, "source": $0.source] }
        var options = sharedOptions
        options["pageNumbers"] = pageNumbers
        let command: [String: Any] = [
            "format": format.rawValue,
            "files": filePayload,
            "options": options
        ]
        return try await callBridgeJSON(
            function: "return await window.frankenBook(command)",
            arguments: ["command": command]
        )
    }

    private func callBridgeJSON<T: Decodable>(
        function: String,
        arguments: [String: Any]
    ) async throws -> T {
        let raw = try await webView.callAsyncJavaScript(
            function,
            arguments: arguments,
            in: nil,
            contentWorld: .page
        )
        guard let json = raw as? String, let data = json.data(using: .utf8) else {
            throw NSError(
                domain: "FrankenMarkdown",
                code: 7,
                userInfo: [NSLocalizedDescriptionKey: "The document engine returned an unreadable response"]
            )
        }
        return try JSONDecoder().decode(T.self, from: data)
    }

    /// Deep links and state restoration can surface the document lab before
    /// WebKit has finished instantiating the bundled WASM module. Keep every
    /// advanced operation behind the same readiness boundary as the preview.
    private func waitForBridgeReady() async throws {
        for _ in 0..<200 {
            switch phase {
            case .loading:
                try await Task.sleep(for: .milliseconds(50))
            case let .failed(message):
                throw NSError(
                    domain: "FrankenMarkdown.Renderer",
                    code: 1,
                    userInfo: [NSLocalizedDescriptionKey: message]
                )
            default:
                return
            }
        }
        throw NSError(
            domain: "FrankenMarkdown.Renderer",
            code: 2,
            userInfo: [NSLocalizedDescriptionKey: "The private Rust renderer did not become ready in time."]
        )
    }

    static let presets: [DocumentPreset] = [
        DocumentPreset(
            id: "living",
            title: "Living Document",
            description: "Core overview with blockquotes, tables, and Rust code",
            markdown: sample
        ),
        DocumentPreset(
            id: "toc",
            title: "Table of Contents",
            description: "Multi-level document demonstrating automatic TOC markers",
            markdown: """
            # Architecture Guide

            [[TOC]]

            ## 1. Engine Core
            Clean-room Rust Markdown renderer compiling to native binaries and browser WASM.

            ### 1.1 Pure Render Path
            No heavy third-party dependencies. Shared AST and unified theme model.

            ### 1.2 Layout & Pagination
            Knuth-Plass optimal line breaking, Liang hyphenation, and baseline grids.

            ## 2. Platform Targets
            - CLI standalone binary (`fmd`)
            - Browser & WebAssembly library
            - Universal iPhone, iPad, and Mac application
            """
        ),
        DocumentPreset(
            id: "math",
            title: "Math & Symbols",
            description: "Mathematical notation and symbols using curated fallback fonts",
            markdown: """
            # Scientific Foundations

            The Euler-Lagrange equation of the second kind:

            $$\\frac{d}{dt} \\left( \\frac{\\partial L}{\\partial \\dot{q}_i} \\right) - \\frac{\\partial L}{\\partial q_i} = 0$$

            ## Quantum Mechanics
            Wave equation:
            $$i\\hbar \\frac{\\partial}{\\partial t} \\Psi(\\mathbf{r},t) = \\hat{H} \\Psi(\\mathbf{r},t)$$

            Symbols: $\\alpha, \\beta, \\gamma, \\delta, \\epsilon, \\theta, \\lambda, \\pi, \\sigma, \\omega, \\rightarrow, \\Leftarrow, \\sum, \\int, \\partial, \\nabla$.
            """
        ),
        DocumentPreset(
            id: "code",
            title: "Code & Syntax",
            description: "Wrapped syntax-highlighted code blocks with line numbers",
            markdown: """
            # Syntax Highlighting Demo

            ```rust
            /// Formats a greeting message for a document author.
            pub fn greet_author(name: &str, documents: usize) -> String {
                format!("Welcome back, {name}! You have {documents} active forge documents.")
            }

            fn main() {
                println!("{}", greet_author("Jeffrey", 42));
            }
            ```

            ```swift
            import SwiftUI

            struct DocumentForgeView: View {
                @State private var text = "# Hello Swift"
                var body: some View {
                    Text(text)
                        .font(.title)
                        .padding()
                }
            }
            ```
            """
        ),
        DocumentPreset(
            id: "executive",
            title: "Executive Decision Memo",
            description: "Board-ready narrative with decision framing, KPI tables, risks, footnotes, and a ninety-day operating plan",
            markdown: """
            ---
            title=Project Prometheus — Decision Memorandum
            author=Strategy & Operations
            lang=en
            toc=true
            toc_depth=3
            ---

            # Project Prometheus

            **Decision memorandum · Confidential draft · 29 August 2026**

            [[TOC]]

            > [!IMPORTANT]
            > **Decision requested.** Approve the staged launch, authorize the first-quarter capacity envelope, and name one executive owner for the adoption target.

            ## Executive summary

            Project Prometheus turns a fragmented, seven-step customer workflow into one private, measurable path. The pilot reduced median completion time by **41%**, improved first-pass success from **68% to 89%**, and produced no critical privacy findings.

            The recommendation is a staged launch: two design partners in September, the existing customer cohort in October, and general availability only after the reliability and support gates below remain green for four consecutive weeks.

            ### The decision in one screen

            | Dimension | Current state | Proposed state | Gate |
            |---|---:|---:|---|
            | Median task time | 18.4 min | **10.8 min** | ≤ 12 min |
            | First-pass success | 68% | **89%** | ≥ 85% |
            | Weekly support load | 126 h | **74 h** | ≤ 80 h |
            | Availability | 99.71% | **99.94%** | ≥ 99.9% |

            ## Why now

            1. The workflow is the highest-volume source of avoidable support demand.
            2. Core technical uncertainty has moved from feasibility to controlled rollout.
            3. Two customers volunteered to serve as reference design partners.
            4. Delaying one quarter carries an estimated opportunity cost of **$1.8–2.4M**.

            ## Operating model

            ```mermaid
            flowchart LR
              A([Customer intent]) --> B[Private intake]
              B --> C{Policy gate}
              C -->|clear| D[Prometheus engine]
              C -->|review| E[Human specialist]
              D --> F[Verified result]
              E --> F
              F --> G([Customer outcome])
              style D fill:#0f766e,color:#ffffff
              style F fill:#16a34a,color:#ffffff
            ```

            ## Economics

            The base case uses conservative adoption and excludes strategic option value.

            $$NPV = \\sum_{t=1}^{12} \\frac{Benefit_t - Cost_t}{(1+r)^t} - Initial\\ Investment$$

            | Scenario | 12-month benefit | Cost | Net value |
            |---|---:|---:|---:|
            | Downside | $1.9M | $1.4M | $0.5M |
            | Base | $3.7M | $1.6M | **$2.1M** |
            | Upside | $6.2M | $2.0M | **$4.2M** |

            ## Principal risks

            > [!WARNING]
            > **Adoption risk.** The product can be technically excellent and still fail if the first-run experience does not establish trust within sixty seconds.

            - **Quality drift:** weekly fixed-corpus evaluation plus automatic rollback.
            - **Support concentration:** progressive eligibility and office-hour coverage.
            - **Scope expansion:** changes require an explicit decision record and owner.
            - **Privacy perception:** show the local data path in-product, not only in policy copy.

            ## Ninety-day plan

            - [ ] **Days 0–15:** harden onboarding, finish accessibility audit, train support.
            - [ ] **Days 16–30:** launch with two design partners and instrument success gates.
            - [ ] **Days 31–60:** expand to the existing cohort after two green weeks.
            - [ ] **Days 61–90:** prepare general availability or publish a no-go memo.

            ## Approval

            | Role | Decision | Date |
            |---|---|---|
            | Executive sponsor | ☐ Approve ☐ Revise ☐ Decline | |
            | Product owner | ☐ Accept operating plan | |
            | Security owner | ☐ Accept residual risk | |

            ---

            [^method]: Pilot figures use the fixed cohort and exclude users who never began the workflow; the launch dashboard will report both intent-to-treat and completed-session views.
            """
        ),
        DocumentPreset(
            id: "research",
            title: "Research White Paper",
            description: "Publication-style abstract, methods, equations, results, limitations, references, and reproducibility checklist",
            markdown: """
            ---
            title=Adaptive Line Breaking Under Mobile Memory Pressure
            author=FrankenMarkdown Research Group
            lang=en
            toc=true
            toc_depth=3
            ---

            # Adaptive Line Breaking Under Mobile Memory Pressure

            *A reproducible systems study of typographic quality, latency, and bounded memory.*

            ## Abstract

            High-quality paragraph composition is often treated as incompatible with interactive mobile editing. We evaluate a dependency-lean Rust renderer that combines Knuth–Plass line breaking, deterministic hyphenation, font subsetting, and streaming document emission. Across a mixed technical corpus, the adaptive pipeline preserves visual quality while reducing peak retained memory and maintaining sub-frame preview latency for ordinary edits.

            **Keywords:** document systems; typography; WebAssembly; bounded memory; deterministic rendering

            [[TOC]]

            ## 1. Introduction

            Reading quality emerges from many small decisions: break opportunities, glue stretch, hyphen penalties, font metrics, and the page builder's treatment of isolated lines. A fast parser alone cannot guarantee a good page.

            Our central question is:

            > Can a single document model deliver responsive preview, self-contained HTML, compact tagged PDF, EPUB, and vector SVG without compromising deterministic output?

            ## 2. Method

            ### 2.1 Objective

            For a paragraph with candidate breakpoints $b_0 \\dots b_n$, the composer minimizes total demerit:

            $$D = \\sum_{i=1}^{n} (1 + badness_i)^2 + penalty_i^2$$

            subject to bounded stretch, shrink, and discretionary hyphenation constraints.

            ### 2.2 Experimental pipeline

            ```mermaid
            sequenceDiagram
              participant E as Editor
              participant P as Parser
              participant L as Layout
              participant R as Renderer
              E->>P: changed source slice
              activate P
              P-->>L: typed AST + diagnostics
              deactivate P
              L->>L: break + paginate
              L-->>R: positioned runs
              R-->>E: HTML / PDF / SVG
              Note over P,R: no network or filesystem in the core
            ```

            ### 2.3 Corpus

            | Stratum | Documents | Median words | Dominant features |
            |---|---:|---:|---|
            | Technical manuals | 240 | 3,810 | code, tables, diagrams |
            | Research articles | 180 | 5,420 | math, citations, footnotes |
            | Product briefs | 320 | 1,170 | callouts, lists, images |
            | Multilingual prose | 160 | 2,060 | de/fr/es/nl hyphenation |

            ## 3. Results

            The preview lane remained interactive throughout the corpus. Streaming page emission reduced retained document state while producing bytes identical to the monolithic oracle.

            | Metric | Baseline | Adaptive | Change |
            |---|---:|---:|---:|
            | Median preview | 14.8 ms | **6.2 ms** | −58% |
            | P95 preview | 42.1 ms | **17.6 ms** | −58% |
            | Peak retained heap | 184 MB | **71 MB** | −61% |
            | Golden mismatches | 0 | **0** | — |

            ## 4. Limitations

            The corpus over-represents Latin-script technical documents. CJK line breaking is included, but complex-script shaping and bidirectional layout require a broader evaluation before general claims are warranted.

            > [!NOTE]
            > Performance numbers describe this fixed corpus and toolchain. They are evidence for the measured system, not a universal hardware claim.

            ## 5. Reproducibility checklist

            - [x] Fixed corpus hashes recorded
            - [x] Deterministic metadata epoch
            - [x] HTML/PDF native↔WASM parity
            - [x] Peak memory sampling protocol published
            - [ ] Independent device replication

            ## References

            1. Knuth, D. E., and Plass, M. F. “Breaking Paragraphs Into Lines.” *Software—Practice & Experience* 11 (1981).
            2. Unicode Consortium. *Unicode Standard Annex #14: Line Breaking Algorithm*.
            3. W3C. *Web Content Accessibility Guidelines (WCAG) 2.2*.
            """
        ),
        DocumentPreset(
            id: "product",
            title: "Product Launch System",
            description: "Narrative brief, personas, requirements, event flow, acceptance criteria, launch scorecard, and incident plan",
            markdown: """
            # Atlas Launch System

            > [!TIP]
            > **North star:** A new user reaches a trustworthy first result in under sixty seconds, without reading documentation.

            [[TOC]]

            ## Product narrative

            Atlas turns a technically powerful workflow into a calm, guided experience. It makes state visible, explains expensive work in plain language, and always leaves the user with a recoverable next action.

            ## Personas and jobs

            Definition List
            : A compact way to preserve shared language across design, engineering, and support.

            **Operator**
            : Needs speed, transparent state, keyboard control, and reproducible output.

            **Explorer**
            : Needs an inviting first run, excellent defaults, and safe experimentation.

            **Reviewer**
            : Needs provenance, comparison, accessibility, and portable artifacts.

            ## Experience architecture

            ```mermaid
            flowchart TD
              A([Open Atlas]) --> B{Existing project?}
              B -->|yes| C[Resume last workspace]
              B -->|no| D[Choose a spectacular template]
              C --> E[Live workspace]
              D --> E
              E --> F{Publish or inspect?}
              F -->|publish| G[Artifact forge]
              F -->|inspect| H[Document intelligence]
              G --> I([Share result])
              H --> E
            ```

            ## Requirements

            | ID | Requirement | Priority | Acceptance evidence |
            |---|---|---|---|
            | R1 | Resume without data loss | Must | force-quit recovery test |
            | R2 | Preview within 100 ms | Must | P95 device trace |
            | R3 | Complete VoiceOver path | Must | audited task script |
            | R4 | Export six formats | Should | artifact conformance matrix |
            | R5 | Delightful processing state | Should | moderated usability study |

            ### Acceptance scenarios

            - [ ] **Given** a large document, **when** the user edits one paragraph, **then** input remains responsive while preview catches up.
            - [ ] **Given** an inaccessible image, **when** the user opens Intelligence, **then** the missing alt text is named with a repair path.
            - [ ] **Given** airplane mode, **when** the user exports, **then** every supported artifact still succeeds.

            ## Launch scorecard

            | Gate | Green | Yellow | Red |
            |---|---:|---:|---:|
            | First-result success | ≥ 90% | 80–89% | < 80% |
            | Crash-free sessions | ≥ 99.8% | 99.3–99.79% | < 99.3% |
            | P95 preview | ≤ 50 ms | 51–100 ms | > 100 ms |
            | Accessibility blockers | 0 | — | ≥ 1 |

            ## Incident plan

            1. Freeze rollout and preserve the failing artifact.
            2. Publish a plain-language status update within thirty minutes.
            3. Restore the last verified configuration.
            4. Reproduce against the immutable input and device profile.
            5. Write the smallest corrective action that prevents recurrence.
            """
        ),
        DocumentPreset(
            id: "mermaid",
            title: "Diagram Atlas",
            description: "A visual systems document showcasing native flowchart, sequence, subgraph, edge-label, note, and activation rendering",
            markdown: """
            # Diagram Atlas

            FrankenMarkdown compiles these diagrams in **pure Rust**. There is no Mermaid JavaScript, browser runtime, or network dependency in the render path.

            ## Multi-stage system

            ```mermaid
            flowchart LR
              subgraph INPUT[Input fabric]
                A([Markdown]) --> B[Scanner]
                B --> C[Block parser]
              end
              subgraph SEMANTIC[Semantic core]
                C --> D{Typed AST}
                D -->|prose| E[Paragraph layout]
                D -->|code| F[Syntax highlighter]
                D -->|diagram| G[Diagram compiler]
              end
              subgraph OUTPUT[Artifact forge]
                E --> H[HTML]
                E --> I[Tagged PDF]
                F --> H
                F --> I
                G --> H
                G --> I
                G --> J[Vector SVG]
              end
              style D fill:#0f766e,color:#fff
              style H fill:#2563eb,color:#fff
              style I fill:#dc2626,color:#fff
              style J fill:#7c3aed,color:#fff
            ```

            ## Private render handshake

            ```mermaid
            sequenceDiagram
              actor U as Author
              participant S as SwiftUI Host
              participant W as WASM Bridge
              participant R as Rust Core
              U->>S: edit Markdown
              S->>W: source + typed options
              activate W
              W->>R: parse once
              activate R
              R->>R: AST → layout → artifact
              R-->>W: bytes + diagnostics
              deactivate R
              W-->>S: preview / share payload
              deactivate W
              S-->>U: visible result
              Note over S,R: all bytes stay on this device
            ```

            ## Decision topology

            ```mermaid
            flowchart TB
              START([Start]) --> SAFE{Input trusted?}
              SAFE -->|no| ESCAPE[Escape raw HTML]
              SAFE -->|yes| PROFILE{Output target}
              ESCAPE --> PROFILE
              PROFILE -->|screen| RESPONSIVE([Self-contained HTML])
              PROFILE -->|print| PAGINATE[(Deterministic PDF)]
              PROFILE -->|archive| EPUB([EPUB 3])
              PROFILE -->|poster| SVG([Path-only SVG])
            ```
            """
        )
    ]

    static let sample = """
    # A living document

    FrankenMarkdown renders this text **entirely on this device** through the same Rust/WASM engine as the command-line tool.

    > Edit on the left. The forged reading view appears on the right.

    | Station | State |
    |---|---:|
    | Source | ready |
    | Rust core | local |
    | Preview | private |

    ```rust
    fn documents_should_feel_alive() -> bool { true }
    ```
    """
}

extension MarkdownRendererModel: WKScriptMessageHandler {
    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        guard let payload = message.body as? [String: Any],
              let type = payload["type"] as? String else { return }
        Task { @MainActor [weak self] in
            guard let self else { return }
            switch type {
            case "ready":
                self.phase = .ready
                self.renderNow()
            case "result":
                guard (payload["requestID"] as? Int) == self.requestID else { return }
                self.elapsedMS = payload["elapsedMS"] as? Double
                self.outputBytes = payload["outputBytes"] as? Int ?? 0
                self.diagnosticCount = payload["diagnosticCount"] as? Int ?? 0
                self.phase = .ready
            case "exportPdfResult":
                guard let req = payload["requestID"] as? Int,
                      let cont = self.pdfContinuations.removeValue(forKey: req) else { return }
                self.phase = .ready
                if let b64 = payload["base64"] as? String,
                   let data = Data(base64Encoded: b64) {
                    let count = payload["byteLength"] as? Int ?? data.count
                    let diag = payload["diagnosticCount"] as? Int ?? 0
                    cont.resume(returning: (data, count, diag))
                } else {
                    cont.resume(throwing: NSError(domain: "FrankenMarkdown", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to decode exported PDF"]))
                }
            case "exportHtmlResult":
                guard let req = payload["requestID"] as? Int,
                      let cont = self.htmlContinuations.removeValue(forKey: req) else { return }
                self.phase = .ready
                if let html = payload["htmlText"] as? String {
                    let count = payload["byteLength"] as? Int ?? html.utf8.count
                    let diag = payload["diagnosticCount"] as? Int ?? 0
                    cont.resume(returning: (html, count, diag))
                } else {
                    cont.resume(throwing: NSError(domain: "FrankenMarkdown", code: 2, userInfo: [NSLocalizedDescriptionKey: "Failed to read exported HTML"]))
                }
            case "exportFailure":
                if let req = payload["requestID"] as? Int {
                    let msg = payload["message"] as? String ?? "Export failed"
                    let err = NSError(domain: "FrankenMarkdown", code: 3, userInfo: [NSLocalizedDescriptionKey: msg])
                    self.pdfContinuations.removeValue(forKey: req)?.resume(throwing: err)
                    self.htmlContinuations.removeValue(forKey: req)?.resume(throwing: err)
                }
                self.phase = .ready
            case "failure":
                self.phase = .failed(payload["message"] as? String ?? "Renderer failed")
            default:
                break
            }
        }
    }
}

extension MarkdownRendererModel: WKNavigationDelegate {
    func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let scheme = navigationAction.request.url?.scheme?.lowercased() else {
            decisionHandler(.cancel)
            return
        }
        decisionHandler(scheme == "frankenmd" || scheme == "about" ? .allow : .cancel)
    }
}

final class FrankenResourceSchemeHandler: NSObject, WKURLSchemeHandler {
    func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url,
              url.host == "bundle",
              let decodedPath = url.path.removingPercentEncoding else {
            fail(urlSchemeTask, code: 400)
            return
        }
        let relativePath = String(decodedPath.drop(while: { $0 == "/" }))
        guard !relativePath.isEmpty,
              !relativePath.split(separator: "/").contains(".."),
              let resourceRoot = Bundle.main.resourceURL?.appendingPathComponent("Renderer", isDirectory: true) else {
            fail(urlSchemeTask, code: 403)
            return
        }
        let candidate = resourceRoot.appendingPathComponent(relativePath).standardizedFileURL
        guard candidate.path.hasPrefix(resourceRoot.standardizedFileURL.path + "/"),
              let bytes = try? Data(contentsOf: candidate) else {
            fail(urlSchemeTask, code: 404)
            return
        }
        let response = URLResponse(
            url: url,
            mimeType: Self.mimeType(for: candidate.pathExtension),
            expectedContentLength: bytes.count,
            textEncodingName: candidate.pathExtension == "html" || candidate.pathExtension == "js" ? "utf-8" : nil
        )
        urlSchemeTask.didReceive(response)
        urlSchemeTask.didReceive(bytes)
        urlSchemeTask.didFinish()
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {}

    private func fail(_ task: WKURLSchemeTask, code: Int) {
        task.didFailWithError(NSError(domain: NSURLErrorDomain, code: code))
    }

    private static func mimeType(for extensionName: String) -> String {
        switch extensionName.lowercased() {
        case "html": "text/html"
        case "js": "text/javascript"
        case "wasm": "application/wasm"
        case "json": "application/json"
        case "css": "text/css"
        default: "application/octet-stream"
        }
    }
}

struct RendererWebView: UIViewRepresentable {
    let webView: WKWebView

    func makeUIView(context: Context) -> WKWebView { webView }
    func updateUIView(_ uiView: WKWebView, context: Context) {}
}
