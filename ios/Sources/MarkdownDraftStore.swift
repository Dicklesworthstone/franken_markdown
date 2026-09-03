import Foundation

struct MarkdownActiveDraft: Codable, Equatable, Sendable {
    static let currentSchema = 1

    let schema: Int
    let savedAtMilliseconds: Int64
    let source: String
    let title: String
    let author: String
    let fontFamily: String
    let rendererDarkMode: String
    let tableOfContents: Bool
    let tableOfContentsDepth: Int
    let pageNumbers: Bool
    let codeLineNumbers: Bool
    let language: String
    let microtypeProtrusion: Bool
    let fitToPages: Int
}

struct MarkdownDraftStore: Sendable {
    enum StoreError: Error, Equatable {
        case invalidDraft
        case oversizedDraft
    }

    static let maximumSourceBytes = 8 * 1_024 * 1_024
    static let maximumEncodedBytes = maximumSourceBytes + 64 * 1_024

    let fileURL: URL

    init(fileURL: URL? = nil) {
        if let fileURL {
            self.fileURL = fileURL
            return
        }
        let applicationSupport = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.temporaryDirectory
        self.fileURL = applicationSupport
            .appendingPathComponent("FrankenMarkdown", isDirectory: true)
            .appendingPathComponent("active-draft.json", isDirectory: false)
    }

    func load() -> MarkdownActiveDraft? {
        guard let values = try? fileURL.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey]),
              values.isRegularFile == true,
              let fileSize = values.fileSize,
              fileSize > 0,
              fileSize <= Self.maximumEncodedBytes,
              let data = try? Data(contentsOf: fileURL, options: .mappedIfSafe),
              let draft = try? JSONDecoder().decode(MarkdownActiveDraft.self, from: data),
              isValid(draft) else {
            return nil
        }
        return draft
    }

    func save(_ draft: MarkdownActiveDraft) throws {
        guard isValid(draft) else { throw StoreError.invalidDraft }
        let data = try JSONEncoder().encode(draft)
        guard data.count <= Self.maximumEncodedBytes else { throw StoreError.oversizedDraft }

        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication]
        )
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var mutableDirectory = directory
        try? mutableDirectory.setResourceValues(values)
        try data.write(to: fileURL, options: [.atomic, .completeFileProtection])
    }

    private func isValid(_ draft: MarkdownActiveDraft) -> Bool {
        draft.schema == MarkdownActiveDraft.currentSchema &&
            draft.source.utf8.count <= Self.maximumSourceBytes &&
            draft.title.utf8.count <= 1_024 &&
            draft.author.utf8.count <= 1_024 &&
            ["sans", "serif"].contains(draft.fontFamily) &&
            ["auto", "disabled"].contains(draft.rendererDarkMode) &&
            (1...6).contains(draft.tableOfContentsDepth) &&
            (0...10_000).contains(draft.fitToPages) &&
            !draft.language.isEmpty && draft.language.utf8.count <= 64
    }
}
