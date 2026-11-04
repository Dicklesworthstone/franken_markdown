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
    private var ownedSource: OwnedSource?

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

    /// A source identity must change on every source assignment. Omitting it
    /// disables reuse. It is never inferred from Swift's Unicode-aware equality.
    func save(_ draft: MarkdownActiveDraft, sourceIdentity: UUID? = nil) async throws {
        let location = fileURL
        let checkpoint = beforePublishing
        let previous = ownedSource
        let written = try await Self.diskAccess.perform {
            try Self.writeSnapshot(
                draft, to: location, sourceIdentity: sourceIdentity,
                reusing: previous, beforePublishing: checkpoint
            )
        }
        // This is only a reuse hint. If actor reentrancy leaves an older hint,
        // the current manifest/signature checks reject it before the next save.
        ownedSource = written
    }

    private struct Header: Decodable {
        let storageSchema: Int?
    }

    private struct Manifest: Codable {
        let storageSchema: Int
        let sourceFileName: String
        let sourceByteCount: Int
        let sourceSHA256: Data
        // Optional for compatibility with earlier schema-2 manifests.
        let sourceIdentity: UUID?
        let draft: MarkdownActiveDraft
    }

    /// Only successful writes by this store can establish reuse ownership.
    /// Loading a manifest never creates this record, including after relaunch.
    /// It retains a small signature and digest, never an older source String.
    private struct OwnedSource: Sendable {
        let identity: UUID
        let fileName: String
        let byteCount: Int
        let digest: Data
        let signature: SourceFileSignature
    }

    private struct SourceFileSignature: Equatable, Sendable {
        let fileSystem: UInt64
        let fileNumber: UInt64
        let byteCount: Int
        let creationDate: Date
        let modificationDate: Date
        let attributeModificationDate: Date
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
        guard isValid(manifest) else { throw StoreError.invalidDraft }
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
        var freshURL = URL(fileURLWithPath: fileURL.path)
        freshURL.removeAllCachedResourceValues()
        let values = try freshURL.resourceValues(
            forKeys: [.fileSizeKey, .isRegularFileKey, .isSymbolicLinkKey]
        )
        guard values.isRegularFile == true, values.isSymbolicLink == false,
              let size = values.fileSize, size > 0, size <= maximumLegacyEncodedBytes else {
            throw StoreError.invalidDraft
        }
        let handle = try FileHandle(forReadingFrom: freshURL)
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
        sourceIdentity: UUID?,
        reusing previous: OwnedSource?,
        beforePublishing: @Sendable () throws -> Void
    ) throws -> OwnedSource? {
        try Task.checkCancellation()
        guard hasValidSettings(draft) else { throw StoreError.invalidDraft }
        let directory = sourceDirectory(for: fileURL)
        let reusable = reusableSource(
            previous, identity: sourceIdentity, manifestURL: fileURL, directory: directory
        )
        try Task.checkCancellation()
        if let reusable {
            let manifest = Manifest(
                storageSchema: storageSchema,
                sourceFileName: reusable.fileName,
                sourceByteCount: reusable.byteCount,
                sourceSHA256: reusable.digest,
                sourceIdentity: reusable.identity,
                draft: draft.replacingSource("")
            )
            // Reusing a known source avoids even a UTF-8 count of a potentially
            // bridged String. Identity, signature and length came from our write.
            try publish(manifest, to: fileURL, beforePublishing: beforePublishing)
            removeObsoleteSources(in: directory, keeping: reusable.fileName)
            return reusable
        }
        guard draft.source.utf8.count <= maximumSourceBytes else { throw StoreError.invalidDraft }
        let manager = FileManager.default
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

        let sourceSHA256 = Data(digest.finalize())
        let manifest = Manifest(
            storageSchema: storageSchema,
            sourceFileName: name,
            sourceByteCount: byteCount,
            sourceSHA256: sourceSHA256,
            sourceIdentity: sourceIdentity,
            draft: draft.replacingSource("")
        )
        let owned: OwnedSource?
        if let sourceIdentity, let signature = sourceFileSignature(at: sourceURL),
           signature.byteCount == byteCount {
            owned = OwnedSource(
                identity: sourceIdentity, fileName: name, byteCount: byteCount,
                digest: sourceSHA256, signature: signature
            )
        } else {
            // Missing filesystem signature support only disables the fast path.
            owned = nil
        }
        try publish(manifest, to: fileURL, beforePublishing: beforePublishing)
        published = true
        // Never throw cancellation or cleanup failures after the commit: the
        // manifest already names a complete, synchronized recovery generation.
        removeObsoleteSources(in: directory, keeping: name)
        return owned
    }

    private static func publish(
        _ manifest: Manifest,
        to fileURL: URL,
        beforePublishing: @Sendable () throws -> Void
    ) throws {
        let data = try JSONEncoder().encode(manifest)
        guard data.count <= maximumMetadataBytes else { throw StoreError.oversizedDraft }
        // A narrow checkpoint permits deterministic interrupted publication
        // tests for both new generations and metadata-only saves.
        try beforePublishing()
        try Task.checkCancellation()
        try data.write(to: fileURL, options: [.atomic, .completeFileProtection])
    }

    private static func reusableSource(
        _ previous: OwnedSource?,
        identity: UUID?,
        manifestURL: URL,
        directory: URL
    ) -> OwnedSource? {
        guard let previous, let identity, previous.identity == identity,
              let data = try? readMetadata(from: manifestURL), data.count <= maximumMetadataBytes,
              let current = try? JSONDecoder().decode(Manifest.self, from: data),
              isValid(current),
              current.sourceIdentity == identity,
              current.sourceFileName == previous.fileName,
              current.sourceByteCount == previous.byteCount,
              current.sourceSHA256 == previous.digest,
              (try? checkSourceDirectory(directory)) != nil else { return nil }
        let sourceURL = directory.appendingPathComponent(previous.fileName, isDirectory: false)
        guard sourceFileSignature(at: sourceURL) == previous.signature else { return nil }
        return previous
    }

    /// Reuse is restricted to a private, immutable generation written by this
    /// live store. Detect replacement, resizing, content writes and attribute
    /// changes without rereading the body. Recovery loads still verify SHA-256;
    /// this metadata signature is not a substitute for that integrity check.
    private static func sourceFileSignature(at url: URL) -> SourceFileSignature? {
        var freshURL = URL(fileURLWithPath: url.path)
        freshURL.removeAllCachedResourceValues()
        guard let values = try? freshURL.resourceValues(
            forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .attributeModificationDateKey]
        ), values.isRegularFile == true, values.isSymbolicLink == false,
              let attributeDate = values.attributeModificationDate,
              let attributes = try? FileManager.default.attributesOfItem(atPath: freshURL.path),
              attributes[.type] as? FileAttributeType == FileAttributeType.typeRegular,
              let fileSystem = attributes[.systemNumber] as? NSNumber,
              let fileNumber = attributes[.systemFileNumber] as? NSNumber,
              let size = attributes[.size] as? NSNumber,
              (0...maximumSourceBytes).contains(size.intValue),
              let created = attributes[.creationDate] as? Date,
              let modified = attributes[.modificationDate] as? Date else { return nil }
        return SourceFileSignature(
            fileSystem: fileSystem.uint64Value, fileNumber: fileNumber.uint64Value,
            byteCount: size.intValue, creationDate: created, modificationDate: modified,
            attributeModificationDate: attributeDate
        )
    }

    private static func isValid(_ manifest: Manifest) -> Bool {
        manifest.storageSchema == storageSchema &&
            isSourceFileName(manifest.sourceFileName) &&
            (0...maximumSourceBytes).contains(manifest.sourceByteCount) &&
            manifest.sourceSHA256.count == 32 &&
            manifest.draft.source.isEmpty &&
            hasValidSettings(manifest.draft)
    }

    private static func sourceDirectory(for fileURL: URL) -> URL {
        fileURL.deletingLastPathComponent().appendingPathComponent(
            fileURL.lastPathComponent + ".sources", isDirectory: true
        )
    }

    private static func checkSourceDirectory(_ directory: URL) throws {
        var freshURL = URL(fileURLWithPath: directory.path, isDirectory: true)
        freshURL.removeAllCachedResourceValues()
        let values = try freshURL.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
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
        draft.source.utf8.count <= maximumSourceBytes && hasValidSettings(draft)
    }

    private static func hasValidSettings(_ draft: MarkdownActiveDraft) -> Bool {
        draft.schema == MarkdownActiveDraft.currentSchema &&
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
