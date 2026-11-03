import Foundation

/// A source-revision snapshot for native status and outline views. Counting and
/// line traversal happen off the main actor, never while SwiftUI builds a frame.
struct MarkdownSourceMetrics: Equatable, Sendable {
    struct Heading: Identifiable, Equatable, Sendable {
        let level: Int
        let title: String
        let lineNumber: Int
        var id: Int { lineNumber }
    }

    let byteCount: Int
    let characterCount: Int
    let wordCount: Int
    let headings: [Heading]

    static let empty = Self(byteCount: 0, characterCount: 0, wordCount: 0, headings: [])

    static func measure(
        _ source: String,
        isCancelled: @escaping () -> Bool = { false }
    ) -> Self? {
        guard !isCancelled() else { return nil }
        var characters = 0
        var words = 0
        var inWord = false
        var scalarCount = 0
        let separators = CharacterSet.whitespacesAndNewlines
        for character in source {
            characters += 1
            for scalar in character.unicodeScalars {
                scalarCount += 1
                if scalarCount & 4_095 == 0, isCancelled() { return nil }
                if separators.contains(scalar) {
                    inWord = false
                } else if !inWord {
                    words += 1
                    inWord = true
                }
            }
        }

        var headings: [Heading] = []
        var lineNumber = 0
        source.enumerateLines { line, stop in
            if isCancelled() {
                stop = true
                return
            }
            lineNumber += 1
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard trimmed.first == "#" else { return }
            // Inspect at most seven markers; an arbitrarily long hash run is
            // not an ATX heading and does not need another full-line scan.
            let level = trimmed.prefix(7).prefix(while: { $0 == "#" }).count
            guard (1 ... 6).contains(level),
                  trimmed.dropFirst(level).first == " " else { return }
            headings.append(Heading(
                level: level,
                title: trimmed.dropFirst(level).trimmingCharacters(in: .whitespaces),
                lineNumber: lineNumber
            ))
        }
        guard !isCancelled() else { return nil }
        return Self(
            byteCount: source.utf8.count,
            characterCount: characters,
            wordCount: words,
            headings: headings
        )
    }

    static func collect(_ source: String) async -> Self? {
        guard !Task.isCancelled else { return nil }
        let worker = Task.detached(priority: .utility) {
            measure(source, isCancelled: { Task.isCancelled })
        }
        return await withTaskCancellationHandler {
            await worker.value
        } onCancel: {
            worker.cancel()
        }
    }
}
