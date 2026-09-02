import Foundation

struct MarkdownSourceDocument: Equatable, Sendable {
    let source: String
    let suggestedTitle: String
}

enum MarkdownSourceLoader {
    static let maximumBytes = 64 * 1024 * 1024

    enum ImportError: LocalizedError, Equatable {
        case notAFileURL
        case tooLarge(maximumBytes: Int)
        case notUTF8

        var errorDescription: String? {
            switch self {
            case .notAFileURL:
                "Choose a Markdown or plain-text file from Files."
            case .tooLarge(let maximumBytes):
                "That document is larger than the supported \(maximumBytes / 1024 / 1024) MB limit."
            case .notUTF8:
                "That document is not valid UTF-8 text."
            }
        }
    }

    static func load(from url: URL) throws -> MarkdownSourceDocument {
        guard url.isFileURL else { throw ImportError.notAFileURL }
        let accessed = url.startAccessingSecurityScopedResource()
        defer {
            if accessed { url.stopAccessingSecurityScopedResource() }
        }

        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let data = try handle.read(upToCount: maximumBytes + 1) ?? Data()
        let source = try decode(data)
        let title = url.deletingPathExtension().lastPathComponent
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return MarkdownSourceDocument(source: source, suggestedTitle: title)
    }

    static func decode(
        _ data: Data,
        maximumBytes: Int = maximumBytes
    ) throws -> String {
        guard data.count <= maximumBytes else {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        guard let source = String(data: data, encoding: .utf8) else {
            throw ImportError.notUTF8
        }
        return source
    }
}
