import SwiftUI

@main
struct FrankenMarkdownApp: App {
    var body: some Scene {
        WindowGroup {
            ForgeView()
                .preferredColorScheme(.dark)
#if targetEnvironment(macCatalyst)
                .frame(minWidth: 760, minHeight: 600)
#endif
        }
#if targetEnvironment(macCatalyst)
        .defaultSize(width: 1220, height: 820)
        .windowResizability(.automatic)
#endif
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Document") {
                    NotificationCenter.default.post(name: .newMarkdownDocument, object: nil)
                }
                .keyboardShortcut("n", modifiers: .command)
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
        }
    }
}

extension Notification.Name {
    static let renderMarkdownNow = Notification.Name("FrankenMarkdown.renderNow")
    static let exportPdfNow = Notification.Name("FrankenMarkdown.exportPdfNow")
    static let exportHtmlNow = Notification.Name("FrankenMarkdown.exportHtmlNow")
    static let newMarkdownDocument = Notification.Name("FrankenMarkdown.newDocument")
}


