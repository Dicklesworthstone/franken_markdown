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
            CommandGroup(replacing: .newItem) {
                Button("New Document") {
                    NotificationCenter.default.post(name: .newMarkdownDocument, object: nil)
                }
                .keyboardShortcut("n", modifiers: .command)

                Button("Open Markdown…") {
                    NotificationCenter.default.post(name: .openMarkdownDocument, object: nil)
                }
            }
            CommandGroup(replacing: .saveItem) {
                Button("Save") {
                    NotificationCenter.default.post(name: .saveMarkdownDocument, object: nil)
                }
                .keyboardShortcut("s", modifiers: .command)

                Button("Save a Copy…") {
                    NotificationCenter.default.post(name: .saveMarkdownDocumentCopy, object: nil)
                }
                .keyboardShortcut("s", modifiers: [.command, .shift])
            }
            CommandMenu("Render") {
                Button("Render Document") {
                    NotificationCenter.default.post(name: .renderMarkdownNow, object: nil)
                }
                .keyboardShortcut("r", modifiers: .command)

                Divider()

                Button("Export PDF...") {
                    NotificationCenter.default.post(name: .exportPdfNow, object: nil)
                }
                .keyboardShortcut("e", modifiers: [.command, .shift])

                Button("Export HTML...") {
                    NotificationCenter.default.post(name: .exportHtmlNow, object: nil)
                }
                .keyboardShortcut("e", modifiers: [.command, .option])
            }
            TextSizeCommands()
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

extension Notification.Name {
    static let renderMarkdownNow = Notification.Name("FrankenMarkdown.renderNow")
    static let exportPdfNow = Notification.Name("FrankenMarkdown.exportPdfNow")
    static let exportHtmlNow = Notification.Name("FrankenMarkdown.exportHtmlNow")
    static let newMarkdownDocument = Notification.Name("FrankenMarkdown.newDocument")
    static let openMarkdownDocument = Notification.Name("FrankenMarkdown.openDocument")
    static let saveMarkdownDocument = Notification.Name("FrankenMarkdown.saveDocument")
    static let saveMarkdownDocumentCopy = Notification.Name("FrankenMarkdown.saveDocumentCopy")
}
