import Foundation
import SwiftUI
import WebKit

enum RenderPhase: Equatable {
    case loading
    case ready
    case rendering
    case failed(String)
}

@MainActor
final class MarkdownRendererModel: NSObject, ObservableObject {
    @Published var source = MarkdownRendererModel.sample
    @Published private(set) var phase: RenderPhase = .loading
    @Published private(set) var elapsedMS: Double?
    @Published private(set) var outputBytes = 0
    @Published private(set) var diagnosticCount = 0

    let webView: WKWebView
    private var requestID = 0
    private var scheduledRender: Task<Void, Never>?

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
        let command: [String: Any] = [
            "requestID": requestID,
            "markdown": source,
            "options": ["darkMode": "auto", "allowRawHtml": false]
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
