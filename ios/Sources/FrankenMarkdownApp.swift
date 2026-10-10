import SwiftUI
import UIKit

@main
struct FrankenMarkdownApp: App {
    var body: some Scene {
        WindowGroup {
            ForgeView()
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
