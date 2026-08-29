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

@MainActor
final class MarkdownRendererModel: NSObject, ObservableObject {
    @Published var source = MarkdownRendererModel.sample
    @Published var fontFamily = "sans"
    @Published var darkMode = "auto"
    @Published var allowRawHtml = false
    @Published var toc = false
    @Published var pageNumbers = false
    @Published var codeLineNumbers = false
    @Published var documentTitle = ""
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

    func scheduleRender() {
        scheduledRender?.cancel()
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
        var options: [String: Any] = [
            "darkMode": validatedDarkMode,
            "allowRawHtml": allowRawHtml,
            "font": fontFamily,
            "fontScale": renderFontScale
        ]
        if !documentTitle.isEmpty {
            options["title"] = documentTitle
        }
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
        var options: [String: Any] = [
            "darkMode": validatedDarkMode,
            "allowRawHtml": allowRawHtml,
            "font": fontFamily,
            "pageNumbers": pageNumbers,
            "codeLineNumbers": codeLineNumbers,
            "fontScale": renderFontScale
        ]
        if !documentTitle.isEmpty {
            options["title"] = documentTitle
        }
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
        var options: [String: Any] = [
            "darkMode": validatedDarkMode,
            "allowRawHtml": allowRawHtml,
            "font": fontFamily,
            "fontScale": renderFontScale
        ]
        if !documentTitle.isEmpty {
            options["title"] = documentTitle
        }
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
