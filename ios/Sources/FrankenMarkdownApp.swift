import SwiftUI
import UIKit

@main
struct FrankenMarkdownApp: App {
    var body: some Scene {
        WindowGroup {
            MarkdownDocumentScene()
                .background(CatalystWindowFreedom())
#if targetEnvironment(macCatalyst)
                .frame(minWidth: 480, minHeight: 420)
#endif
        }
#if targetEnvironment(macCatalyst)
        .defaultSize(width: 1220, height: 820)
        .windowResizability(.contentMinSize)
#endif
        .commands {
            MarkdownCommands()
            TextSizeCommands()
        }
    }
}

struct MarkdownSceneOpenRequest: Equatable {
    let id = UUID()
    let url: URL
}

/// Two live windows must never share a namespace, even if their restored scene
/// metadata contains the same identifier. Weak ownership survives ordinary view
/// re-evaluation and releases naturally when SwiftUI destroys the scene model.
@MainActor
final class MarkdownSceneRecoveryOwner: ObservableObject {
    private struct WeakOwner {
        weak var value: MarkdownSceneRecoveryOwner?
    }

    private static var active: [MarkdownSceneIdentity: WeakOwner] = [:]
    private(set) var identity: MarkdownSceneIdentity?

    static var hasActiveScenes: Bool { active.values.contains { $0.value != nil } }

    static func isActive(_ identity: MarkdownSceneIdentity) -> Bool {
        active[identity]?.value != nil
    }

    func claim(storedIdentifier: String) -> MarkdownSceneIdentity {
        if let identity { return identity }
        Self.active = Self.active.filter { $0.value.value != nil }
        var candidate = MarkdownSceneIdentity(storedValue: storedIdentifier) ?? MarkdownSceneIdentity()
        while Self.active[candidate]?.value != nil { candidate = MarkdownSceneIdentity() }
        identity = candidate
        Self.active[candidate] = WeakOwner(value: self)
        return candidate
    }
}

/// SceneStorage is restored before this view's task starts. Constructing the
/// StateObjects in a child only after resolution avoids loading or overwriting
/// the legacy singleton during an earlier, unscoped initialization.
private struct MarkdownDocumentScene: View {
    @SceneStorage("frankenmarkdown.documentScene.v1") private var storedIdentifier = ""
    @StateObject private var owner = MarkdownSceneRecoveryOwner()
    @State private var recovery: MarkdownSceneRecovery?
    @State private var openRequest: MarkdownSceneOpenRequest?
    @State private var recoveryError: String?
    @State private var savedDrafts: [MarkdownSceneDraftSummary]?
    @State private var startsNewDocument = false
    @State private var restoresLegacy = true
    @State private var retry = 0

    var body: some View {
        Group {
            if let recovery {
                ForgeView(recovery: recovery, openRequest: openRequest)
                    .id(recovery.identity)
            } else if let savedDrafts {
                VStack(spacing: 12) {
                    Text("Recover a saved document")
                        .font(.headline)
                    Text("This window has no restored document. Your saved drafts are available below.")
                    List(savedDrafts) { draft in
                        Button {
                            guard !MarkdownSceneRecoveryOwner.isActive(draft.id) else {
                                self.savedDrafts = savedDrafts.filter { $0.id != draft.id }
                                return
                            }
                            // Reserve synchronously at the selection event;
                            // another window may choose before .task resumes.
                            storedIdentifier = owner.claim(storedIdentifier: draft.id.storageValue).storageValue
                            self.savedDrafts = nil
                            retry += 1
                        } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(draft.title.isEmpty ? "Untitled document" : draft.title)
                                Text(Date(timeIntervalSince1970: Double(draft.savedAtMilliseconds) / 1_000), style: .date)
                                    .font(.caption)
                            }
                        }
                    }
                    Button("New Document") {
                        startsNewDocument = true
                        self.savedDrafts = nil
                        retry += 1
                    }
                }
                .padding()
            } else if let recoveryError {
                VStack(spacing: 12) {
                    Text("Could not open this window's recovery")
                        .font(.headline)
                    Text(recoveryError)
                    Text("Your saved drafts have been kept. Try again when storage is available.")
                    Button("Try Again") { retry += 1 }
                    if restoresLegacy {
                        Button("Open Without Older Recovery") {
                            restoresLegacy = false
                            retry += 1
                        }
                    }
                }
                .padding()
            } else {
                ProgressView("Opening document…")
            }
        }
        .onOpenURL { url in
            openRequest = MarkdownSceneOpenRequest(url: url)
            if recovery == nil, owner.identity == nil,
               MarkdownSceneIdentity(storedValue: storedIdentifier) == nil {
                // An explicit file/deep-link request may arrive before scene
                // preparation; retain it and bypass the optional draft picker.
                startsNewDocument = true
                savedDrafts = nil
                retry += 1
            }
        }
        .task(id: retry) {
            guard recovery == nil else { return }
            recoveryError = nil
            do {
                if owner.identity == nil, !startsNewDocument,
                   MarkdownSceneIdentity(storedValue: storedIdentifier) == nil,
                   !MarkdownSceneRecoveryOwner.hasActiveScenes {
                    let summaries = try await MarkdownDraftStore.savedSceneDrafts()
                    try Task.checkCancellation()
                    let available = summaries.filter { !MarkdownSceneRecoveryOwner.isActive($0.id) }
                    if !available.isEmpty {
                        savedDrafts = available
                        return
                    }
                }
                let identity = owner.claim(storedIdentifier: startsNewDocument ? "" : storedIdentifier)
                storedIdentifier = identity.storageValue
                let prepared = try await MarkdownDraftStore.prepareSceneRecovery(
                    for: identity, restoreLegacy: restoresLegacy
                )
                try Task.checkCancellation()
                recovery = prepared
            } catch is CancellationError {
                // A recreated scene retries with the same persisted identity.
            } catch {
                recoveryError = error.localizedDescription
            }
        }
    }
}

/// Commands belong to the active document scene, even when its editor does not
/// hold keyboard focus. Broadcasting notifications would edit every open window.
struct MarkdownSceneActions {
    var newDocument: () -> Void
    var openDocument: () -> Void
    var saveDocument: () -> Void
    var saveCopy: () -> Void
    var render: () -> Void
    var exportPDF: () -> Void
    var exportHTML: () -> Void
    var canSave: Bool
}

private struct MarkdownSceneActionsKey: FocusedValueKey {
    typealias Value = MarkdownSceneActions
}

extension FocusedValues {
    var markdownSceneActions: MarkdownSceneActions? {
        get { self[MarkdownSceneActionsKey.self] }
        set { self[MarkdownSceneActionsKey.self] = newValue }
    }
}

private struct MarkdownCommands: Commands {
    @FocusedValue(\.markdownSceneActions) private var actions

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Document") { actions?.newDocument() }
                .keyboardShortcut("n", modifiers: .command)
                .disabled(actions == nil)

            Button("Open Markdown…") { actions?.openDocument() }
                .keyboardShortcut("o", modifiers: .command)
                .disabled(actions == nil)
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save") { actions?.saveDocument() }
                .keyboardShortcut("s", modifiers: .command)
                .disabled(actions?.canSave != true)

            Button("Save a Copy…") { actions?.saveCopy() }
                .keyboardShortcut("s", modifiers: [.command, .shift])
                .disabled(actions == nil)
        }
        CommandMenu("Render") {
            Button("Render Document") { actions?.render() }
                .keyboardShortcut("r", modifiers: .command)
                .disabled(actions == nil)

            Divider()

            Button("Export PDF...") { actions?.exportPDF() }
                .keyboardShortcut("e", modifiers: [.command, .shift])
                .disabled(actions == nil)

            Button("Export HTML...") { actions?.exportHTML() }
                .keyboardShortcut("e", modifiers: [.command, .option])
                .disabled(actions == nil)
        }
    }
}

private struct TextSizeCommands: Commands {
    @AppStorage(Lab.textScaleStorageKey) private var textScale = Lab.defaultTextScale

    var body: some Commands {
        CommandMenu("Text Size") {
            Button("Increase Text Size") {
                textScale = Lab.adjustedTextScale(textScale, steps: 1)
            }
            .keyboardShortcut("+", modifiers: .command)
            .disabled(textScale >= Lab.maximumTextScale)

            Button("Decrease Text Size") {
                textScale = Lab.adjustedTextScale(textScale, steps: -1)
            }
            .keyboardShortcut("-", modifiers: .command)
            .disabled(textScale <= Lab.minimumTextScale)

            Divider()

            Button("Actual Text Size") {
                textScale = Lab.defaultTextScale
            }
            .keyboardShortcut("0", modifiers: .command)
        }
    }
}

private struct CatalystWindowFreedom: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> Controller { Controller() }
    func updateUIViewController(_ controller: Controller, context: Context) { controller.configure() }

    final class Controller: UIViewController {
        override func viewDidAppear(_ animated: Bool) {
            super.viewDidAppear(animated)
            configure()
        }

        override func viewDidLayoutSubviews() {
            super.viewDidLayoutSubviews()
            configure()
        }

        func configure() {
#if targetEnvironment(macCatalyst)
            guard let restrictions = view.window?.windowScene?.sizeRestrictions else { return }
            restrictions.minimumSize = CGSize(width: 480, height: 420)
            restrictions.maximumSize = CGSize(width: 10_000, height: 10_000)
#endif
        }
    }
}
