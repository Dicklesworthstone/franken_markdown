import CryptoKit
import Foundation

struct MarkdownActiveDraft: Codable, Equatable, Sendable {
    static let currentSchema = 1
    static let maximumCustomCSSBytes = 256 * 1_024

    let schema: Int
    let savedAtMilliseconds: Int64
    let documentIdentity: UUID?
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
    let customizePDFTypography: Bool?
    let pdfBaseFontSize: Double?
    let pdfHeadingScale: Double?
    let pdfTableFontSize: Double?
    let customCSS: String?

    fileprivate func replacingSource(_ source: String) -> Self {
        Self(
            schema: schema,
            savedAtMilliseconds: savedAtMilliseconds,
            documentIdentity: documentIdentity,
            source: source,
            title: title,
            author: author,
            fontFamily: fontFamily,
            rendererDarkMode: rendererDarkMode,
            tableOfContents: tableOfContents,
            tableOfContentsDepth: tableOfContentsDepth,
            pageNumbers: pageNumbers,
            codeLineNumbers: codeLineNumbers,
            language: language,
            microtypeProtrusion: microtypeProtrusion,
            fitToPages: fitToPages,
            customizePDFTypography: customizePDFTypography,
            pdfBaseFontSize: pdfBaseFontSize,
            pdfHeadingScale: pdfHeadingScale,
            pdfTableFontSize: pdfTableFontSize,
            customCSS: customCSS
        )
    }
}

/// A recovery snapshot is a small atomic manifest plus an immutable UTF-8 file.
/// Source encoding, hashing, and decoding use bounded chunks off the main actor.
actor MarkdownDraftStore {
    enum StoreError: Error, Equatable {
        case invalidDraft
        case oversizedDraft
    }

    static let maximumSourceBytes = MarkdownSourceLoader.maximumBytes
    static let maximumLegacySourceBytes = 8 * 1_024 * 1_024
    static let maximumLegacyEncodedBytes = maximumLegacySourceBytes
        + MarkdownActiveDraft.maximumCustomCSSBytes
        + 64 * 1_024
    // A control character can occupy six bytes after JSON escaping. Only safe
    // presentation settings, never the document body, enter the new JSON file.
    static let maximumMetadataBytes = 6 * MarkdownActiveDraft.maximumCustomCSSBytes
        + 64 * 1_024

    private static let storageSchema = 2
    private static let diskAccess = MarkdownDraftDiskAccess()

    nonisolated let fileURL: URL
    private let beforePublishing: @Sendable () throws -> Void

    init(
        fileURL: URL? = nil,
        beforePublishing: @escaping @Sendable () throws -> Void = {}
    ) {
        self.beforePublishing = beforePublishing
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

    func load() async -> MarkdownActiveDraft? {
        let location = fileURL
        return await Self.diskAccess.perform {
            try? Self.readSnapshot(from: location)
        }
    }

    func save(_ draft: MarkdownActiveDraft) async throws {
        let location = fileURL
        let checkpoint = beforePublishing
        try await Self.diskAccess.perform {
            try Self.writeSnapshot(draft, to: location, beforePublishing: checkpoint)
        }
    }

    private struct Header: Decodable {
        let storageSchema: Int?
    }

    private struct Manifest: Codable {
        let storageSchema: Int
        let sourceFileName: String
        let sourceByteCount: Int
        let sourceSHA256: Data
        let draft: MarkdownActiveDraft
    }

    private static func readSnapshot(from fileURL: URL) throws -> MarkdownActiveDraft {
        try Task.checkCancellation()
        let data = try readMetadata(from: fileURL)
        let decoder = JSONDecoder()
        let header = try decoder.decode(Header.self, from: data)
        guard let format = header.storageSchema else {
            // Older installations stored the body in JSON. Preserve their
            // original limits; migration occurs only after a successful save.
            let draft = try decoder.decode(MarkdownActiveDraft.self, from: data)
            guard isValid(draft, maximumSourceBytes: maximumLegacySourceBytes) else {
                throw StoreError.invalidDraft
            }
            try Task.checkCancellation()
            return draft
        }
        guard format == storageSchema, data.count <= maximumMetadataBytes else {
            throw StoreError.invalidDraft
        }
        let manifest = try decoder.decode(Manifest.self, from: data)
        guard isSourceFileName(manifest.sourceFileName),
              (0...maximumSourceBytes).contains(manifest.sourceByteCount),
              manifest.sourceSHA256.count == 32,
              manifest.draft.source.isEmpty,
              isValid(manifest.draft) else {
            throw StoreError.invalidDraft
        }
        let directory = sourceDirectory(for: fileURL)
        try checkSourceDirectory(directory)
        let sourceURL = directory.appendingPathComponent(manifest.sourceFileName, isDirectory: false)
        let values = try sourceURL.resourceValues(
            forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]
        )
        guard values.isRegularFile == true, values.isSymbolicLink == false,
              values.fileSize == manifest.sourceByteCount else {
            throw StoreError.invalidDraft
        }
        let handle = try FileHandle(forReadingFrom: sourceURL)
        defer { try? handle.close() }
        let loaded = try MarkdownSourceLoader.readSource(
            maximumBytes: manifest.sourceByteCount,
            capacityHint: manifest.sourceByteCount,
            stripInitialByteOrderMark: false
        ) { count in
            try handle.read(upToCount: count) ?? Data()
        }
        guard loaded.diskFingerprint.byteCount == manifest.sourceByteCount,
              loaded.diskFingerprint.digest == manifest.sourceSHA256 else {
            throw StoreError.invalidDraft
        }
        try Task.checkCancellation()
        return manifest.draft.replacingSource(loaded.source)
    }

    /// Stat is an early rejection, not the allocation bound: a file that grows
    /// during a read must still stop at the legacy metadata budget plus a probe.
    private static func readMetadata(from fileURL: URL) throws -> Data {
        let values = try fileURL.resourceValues(
            forKeys: [.fileSizeKey, .isRegularFileKey, .isSymbolicLinkKey]
        )
        guard values.isRegularFile == true, values.isSymbolicLink == false,
              let size = values.fileSize, size > 0, size <= maximumLegacyEncodedBytes else {
            throw StoreError.invalidDraft
        }
        let handle = try FileHandle(forReadingFrom: fileURL)
        defer { try? handle.close() }
        var data = Data()
        data.reserveCapacity(size)
        while true {
            try Task.checkCancellation()
            let remaining = maximumLegacyEncodedBytes - data.count
            let requested = remaining == 0 ? 1 : min(MarkdownSourceLoader.ioChunkBytes, remaining)
            let chunk = try handle.read(upToCount: requested) ?? Data()
            if chunk.isEmpty { break }
            guard chunk.count <= remaining else { throw StoreError.oversizedDraft }
            data.append(chunk)
        }
        guard !data.isEmpty else { throw StoreError.invalidDraft }
        return data
    }

    private static func writeSnapshot(
        _ draft: MarkdownActiveDraft,
        to fileURL: URL,
        beforePublishing: @Sendable () throws -> Void
    ) throws {
        try Task.checkCancellation()
        guard isValid(draft) else { throw StoreError.invalidDraft }
        let manager = FileManager.default
        let directory = sourceDirectory(for: fileURL)
        try manager.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication]
        )
        try checkSourceDirectory(directory)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var parent = fileURL.deletingLastPathComponent()
        try? parent.setResourceValues(values)
        var protectedDirectory = directory
        try? protectedDirectory.setResourceValues(values)

        let name = UUID().uuidString + ".utf8"
        let sourceURL = directory.appendingPathComponent(name, isDirectory: false)
        // Only an empty Data value is created here. Exclusive creation makes
        // this generation ours to remove on any failure before publication.
        try Data().write(to: sourceURL, options: [.withoutOverwriting, .completeFileProtection])
        var published = false
        defer {
            if !published { try? manager.removeItem(at: sourceURL) }
        }
        let handle = try FileHandle(forWritingTo: sourceURL)
        defer { try? handle.close() }
        var digest = SHA256()
        var byteCount = 0
        try MarkdownSourceLoader.forEachUTF8Chunk(draft.source) { chunk in
            try Task.checkCancellation()
            try handle.write(contentsOf: chunk)
            digest.update(data: chunk)
            byteCount += chunk.count
        }
        try Task.checkCancellation()
        try handle.synchronize()
        try handle.close()

        let manifest = Manifest(
            storageSchema: storageSchema,
            sourceFileName: name,
            sourceByteCount: byteCount,
            sourceSHA256: Data(digest.finalize()),
            draft: draft.replacingSource("")
        )
        let data = try JSONEncoder().encode(manifest)
        guard data.count <= maximumMetadataBytes else { throw StoreError.oversizedDraft }
        // A narrow checkpoint also permits deterministic interrupted-commit
        // tests after the complete source is on disk but still unreferenced.
        try beforePublishing()
        try Task.checkCancellation()
        try data.write(to: fileURL, options: [.atomic, .completeFileProtection])
        published = true
        // Never throw cancellation or cleanup failures after the commit: the
        // manifest already names a complete, synchronized recovery generation.
        removeObsoleteSources(in: directory, keeping: name)
    }

    private static func sourceDirectory(for fileURL: URL) -> URL {
        fileURL.deletingLastPathComponent().appendingPathComponent(
            fileURL.lastPathComponent + ".sources", isDirectory: true
        )
    }

    private static func checkSourceDirectory(_ directory: URL) throws {
        let values = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink == false else {
            throw StoreError.invalidDraft
        }
    }

    private static func isSourceFileName(_ name: String) -> Bool {
        guard name.hasSuffix(".utf8"), name.utf8.count == 41,
              let identifier = UUID(uuidString: String(name.dropLast(5))) else { return false }
        return name == identifier.uuidString + ".utf8"
    }

    private static func removeObsoleteSources(in directory: URL, keeping name: String) {
        guard let files = try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey]
        ) else { return }
        for file in files where file.lastPathComponent != name && isSourceFileName(file.lastPathComponent) {
            guard let values = try? file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]),
                  values.isRegularFile == true, values.isSymbolicLink == false else { continue }
            try? FileManager.default.removeItem(at: file)
        }
    }

    private static func isValid(
        _ draft: MarkdownActiveDraft,
        maximumSourceBytes: Int = maximumSourceBytes
    ) -> Bool {
        draft.schema == MarkdownActiveDraft.currentSchema &&
            draft.source.utf8.count <= maximumSourceBytes &&
            draft.title.utf8.count <= 1_024 &&
            draft.author.utf8.count <= 1_024 &&
            ["sans", "serif"].contains(draft.fontFamily) &&
            ["auto", "disabled"].contains(draft.rendererDarkMode) &&
            (1...6).contains(draft.tableOfContentsDepth) &&
            (0...10_000).contains(draft.fitToPages) &&
            (draft.pdfBaseFontSize.map { (6...24).contains($0) } ?? true) &&
            (draft.pdfHeadingScale.map { (1.05...2).contains($0) } ?? true) &&
            (draft.pdfTableFontSize.map { (5...24).contains($0) } ?? true) &&
            (draft.customCSS?.utf8.count ?? 0) <= MarkdownActiveDraft.maximumCustomCSSBytes &&
            !draft.language.isEmpty && draft.language.utf8.count <= 64
    }
}

/// Model/scene restoration may create several store instances for one path.
/// Serialize whole operations across them: deleting an obsolete generation
/// must never race a reader or a writer that has not published its manifest.
private actor MarkdownDraftDiskAccess {
    func perform<Result: Sendable>(_ operation: @Sendable () throws -> Result) rethrows -> Result {
        try operation()
    }
}
