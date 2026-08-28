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
            CommandMenu("Render") {
                Button("Render Document") {
                    NotificationCenter.default.post(name: .renderMarkdownNow, object: nil)
                }
                .keyboardShortcut("r", modifiers: .command)
            }
        }
    }
}

extension Notification.Name {
    static let renderMarkdownNow = Notification.Name("FrankenMarkdown.renderNow")
}

