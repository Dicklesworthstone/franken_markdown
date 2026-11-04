import CryptoKit
import Foundation
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    static let frankenMarkdownSource = UTType(
        importedAs: "net.daringfireball.markdown",
        conformingTo: .plainText
    )
}

struct MarkdownSourceFile: FileDocument {
    static var readableContentTypes: [UTType] { [.frankenMarkdownSource, .plainText] }

    let source: String

    init(source: String) {
        self.source = source
    }

    init(configuration: ReadConfiguration) throws {
        guard let data = configuration.file.regularFileContents else {
            throw MarkdownSourceLoader.ImportError.notAFileURL
        }
        source = try MarkdownSourceLoader.decode(data)
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: try MarkdownSourceLoader.encode(source))
    }
}

struct MarkdownSourceDocument: Equatable, Sendable {
    let url: URL
    let bookmarkData: Data
    let source: String
    let suggestedTitle: String
    let diskFingerprint: MarkdownSourceFingerprint
    let sourceDigest: Data

    var displayName: String { url.lastPathComponent }
}

/// Conflict detection needs only the original bytes' identity, not another
/// complete copy of a large document beside the editable String.
struct MarkdownSourceFingerprint: Equatable, Sendable {
    let byteCount: Int
    let digest: Data
    let hasByteOrderMark: Bool
}

struct MarkdownRecentDocument: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let bookmarkData: Data
    let displayName: String
    let lastOpenedAt: Date
}

private struct MarkdownActiveDocumentReference: Codable, Equatable, Sendable {
    let documentIdentity: UUID
    let bookmarkData: Data
    let displayName: String
    let baselineSourceDigest: Data
    let baselineDiskDigest: Data
}

struct MarkdownRestoredDocument: Equatable, Sendable {
    let document: MarkdownSourceDocument
    let documentIdentity: UUID
}

enum MarkdownDocumentRestoration: Equatable, Sendable {
    case none
    case fileVersion(MarkdownRestoredDocument)
    case recoveredEdits(MarkdownRestoredDocument)
    case conflict(MarkdownRestoredDocument)
    case unassociatedDraft(MarkdownRestoredDocument)
}

enum MarkdownDocumentAttention: Equatable {
    case changedOnDisk
    case recoveryConflict
    case unavailable
}

@MainActor
final class MarkdownDocumentSession: ObservableObject {
    static let recentsStorageKey = "frankenmarkdown.recentSourceDocuments.v1"
    static let activeDocumentStorageKey = "frankenmarkdown.activeSourceDocument.v1"
    static let maximumRecentDocuments = 6

    @Published private(set) var currentDocument: MarkdownSourceDocument?
    @Published private(set) var recentDocuments: [MarkdownRecentDocument]
    @Published private(set) var isSaving = false
    @Published private(set) var attention: MarkdownDocumentAttention?
    @Published private(set) var currentDocumentIdentity: UUID?

    private var untitledBaseline: String
    private var activeDocumentReference: MarkdownActiveDocumentReference?
    private var restorationDisplayName: String?
    private let defaults: UserDefaults
    private let openDocument: @Sendable (URL) async throws -> MarkdownSourceDocument
    private let saveDocument: @Sendable (String, MarkdownSourceDocument) async throws -> MarkdownSourceDocument
    private var requestGeneration = UUID()
    private var explicitDocumentRequested = false
    private(set) var documentGeneration = UUID()

    init(
        initialSource: String,
        defaults: UserDefaults = .standard,
        openDocument: @escaping @Sendable (URL) async throws -> MarkdownSourceDocument = {
            try await MarkdownSourceLoader.open(from: $0)
        },
        saveDocument: @escaping @Sendable (String, MarkdownSourceDocument) async throws -> MarkdownSourceDocument = {
            try await MarkdownSourceLoader.save($0, replacing: $1)
        }
    ) {
        untitledBaseline = initialSource
        self.defaults = defaults
        self.openDocument = openDocument
        self.saveDocument = saveDocument
        recentDocuments = Self.loadRecents(from: defaults)
        activeDocumentReference = Self.loadActiveDocument(from: defaults)
        restorationDisplayName = activeDocumentReference?.displayName
    }

    var displayName: String {
        currentDocument?.displayName ?? restorationDisplayName ?? "Untitled.md"
    }
    var hasCurrentDocument: Bool { currentDocument != nil }

    func isDirty(source: String) -> Bool {
        source != (currentDocument?.source ?? untitledBaseline)
    }

    /// An explicit picker, New, or file-open request supersedes automatic launch
    /// restoration and any older read. The underlying coordinated I/O can finish
    /// safely, but its result must no longer replace the active document.
    func cancelPendingDocumentRequests() {
        requestGeneration = UUID()
        explicitDocumentRequested = true
    }

    /// Reserve the request synchronously at the UI event, before a Task can
    /// suspend. A later event must win even if an earlier Task starts late.
    func beginDocumentRequest() -> UUID {
        cancelPendingDocumentRequests()
        return requestGeneration
    }

    func isCurrentDocumentRequest(_ request: UUID) -> Bool {
        request == requestGeneration
    }

    private func beginDocumentTransition() {
        cancelPendingDocumentRequests()
        documentGeneration = UUID()
        isSaving = false
    }

    func beginUntitled(source: String) {
        beginDocumentTransition()
        currentDocument = nil
        currentDocumentIdentity = nil
        untitledBaseline = source
        attention = nil
        restorationDisplayName = nil
        activeDocumentReference = nil
        defaults.removeObject(forKey: Self.activeDocumentStorageKey)
    }

    func adopt(
        _ document: MarkdownSourceDocument,
        documentIdentity: UUID = UUID()
    ) {
        beginDocumentTransition()
        currentDocument = document
        currentDocumentIdentity = documentIdentity
        attention = nil
        restorationDisplayName = nil
        recordActive(document, documentIdentity: documentIdentity)
        recordRecent(document)
    }

    func adoptRecoveredEdits(
        from document: MarkdownSourceDocument,
        documentIdentity: UUID,
        changedOnDisk: Bool
    ) {
        beginDocumentTransition()
        currentDocument = document
        currentDocumentIdentity = documentIdentity
        attention = changedOnDisk ? .changedOnDisk : nil
        restorationDisplayName = nil
        if !changedOnDisk {
            recordActive(document, documentIdentity: documentIdentity)
        }
    }

    func adoptUnassociatedDraft(
        while retaining: MarkdownSourceDocument,
        documentIdentity: UUID
    ) {
        beginDocumentTransition()
        currentDocument = retaining
        currentDocumentIdentity = documentIdentity
        attention = .recoveryConflict
        restorationDisplayName = nil
    }

    func restoreActiveDocument(
        recoveredSource: String?,
        recoveredDocumentIdentity: UUID?,
        isCurrentSource: () -> Bool = { true }
    ) async throws -> MarkdownDocumentRestoration {
        guard !explicitDocumentRequested, let reference = activeDocumentReference else { return .none }
        let request = requestGeneration
        do {
            let url = try MarkdownSourceLoader.resolveBookmark(reference.bookmarkData)
            let document = try await openDocument(url)
            // Read the live buffer before either returning a restored file or
            // mutating its error state. SwiftUI onChange delivery can lag behind
            // a user's edit and must not decide which asynchronous result wins.
            guard request == requestGeneration, isCurrentSource() else { return .none }
            let restored = MarkdownRestoredDocument(
                document: document,
                documentIdentity: reference.documentIdentity
            )
            guard let recoveredSource else { return .fileVersion(restored) }
            guard recoveredDocumentIdentity == reference.documentIdentity else {
                return .unassociatedDraft(restored)
            }

            try Task.checkCancellation()
            let digestWorker = Task.detached(priority: .utility) {
                try MarkdownSourceLoader.sourceDigest(recoveredSource)
            }
            let recoveredSourceDigest = try await withTaskCancellationHandler {
                try await digestWorker.value
            } onCancel: {
                digestWorker.cancel()
            }
            guard request == requestGeneration, isCurrentSource() else { return .none }
            if recoveredSourceDigest == reference.baselineSourceDigest {
                return .fileVersion(restored)
            }
            if document.diskFingerprint.digest == reference.baselineDiskDigest {
                return .recoveredEdits(restored)
            }
            return .conflict(restored)
        } catch {
            guard request == requestGeneration, isCurrentSource() else { return .none }
            if error is CancellationError { return .none }
            attention = .unavailable
            restorationDisplayName = reference.displayName
            throw error
        }
    }

    func open(from url: URL, request: UUID? = nil) async throws -> MarkdownSourceDocument? {
        let request = request ?? beginDocumentRequest()
        guard isCurrentDocumentRequest(request) else { return nil }
        return try await readRequestedDocument(from: url, request: request)
    }

    func openRecent(
        _ recent: MarkdownRecentDocument,
        request: UUID? = nil
    ) async throws -> MarkdownSourceDocument? {
        let request = request ?? beginDocumentRequest()
        guard isCurrentDocumentRequest(request) else { return nil }
        let url = try MarkdownSourceLoader.resolveBookmark(recent.bookmarkData)
        return try await readRequestedDocument(from: url, request: request)
    }

    private func readRequestedDocument(
        from url: URL,
        request: UUID
    ) async throws -> MarkdownSourceDocument? {
        do {
            let document = try await openDocument(url)
            guard request == requestGeneration else { return nil }
            return document
        } catch {
            // A failed older open must not cover the newly selected document
            // with an unrelated error alert either.
            guard request == requestGeneration else { return nil }
            throw error
        }
    }

    func save(source: String) async throws {
        guard let currentDocument else { throw MarkdownSourceLoader.DocumentError.noCurrentDocument }
        if attention == .changedOnDisk {
            throw MarkdownSourceLoader.DocumentError.changedOnDisk
        }
        if attention == .recoveryConflict {
            throw MarkdownSourceLoader.DocumentError.recoveryConflict
        }
        guard !isSaving else { return }
        let generation = documentGeneration
        let identity = currentDocumentIdentity ?? UUID()
        isSaving = true
        defer {
            if generation == documentGeneration { isSaving = false }
        }
        do {
            let saved = try await saveDocument(source, currentDocument)
            // Saving the previous file may finish after New/Open. Its disk
            // write still belongs to that file; its session state does not.
            guard generation == documentGeneration else { return }
            self.currentDocument = saved
            attention = nil
            recordActive(
                saved,
                documentIdentity: identity
            )
            recordRecent(saved)
        } catch {
            if generation == documentGeneration {
                if error as? MarkdownSourceLoader.DocumentError == .changedOnDisk {
                    attention = .changedOnDisk
                } else if Self.isUnavailableFileError(error) {
                    attention = .unavailable
                }
            }
            throw error
        }
    }

    func suggestedFilename() -> String {
        currentDocument?.url.deletingPathExtension().lastPathComponent ?? "Untitled"
    }

    private func recordRecent(_ document: MarkdownSourceDocument) {
        let recent = MarkdownRecentDocument(
            id: recentDocuments.first(where: { represents($0, url: document.url) })?.id ?? UUID(),
            bookmarkData: document.bookmarkData,
            displayName: document.displayName,
            lastOpenedAt: .now
        )
        recentDocuments.removeAll { represents($0, url: document.url) }
        recentDocuments.insert(recent, at: 0)
        if recentDocuments.count > Self.maximumRecentDocuments {
            recentDocuments = Array(recentDocuments.prefix(Self.maximumRecentDocuments))
        }
        if let encoded = try? JSONEncoder().encode(recentDocuments) {
            defaults.set(encoded, forKey: Self.recentsStorageKey)
        }
    }

    private func recordActive(
        _ document: MarkdownSourceDocument,
        documentIdentity: UUID
    ) {
        let reference = MarkdownActiveDocumentReference(
            documentIdentity: documentIdentity,
            bookmarkData: document.bookmarkData,
            displayName: document.displayName,
            baselineSourceDigest: document.sourceDigest,
            baselineDiskDigest: document.diskFingerprint.digest
        )
        activeDocumentReference = reference
        if let encoded = try? JSONEncoder().encode(reference) {
            defaults.set(encoded, forKey: Self.activeDocumentStorageKey)
        }
    }

    private func represents(_ recent: MarkdownRecentDocument, url: URL) -> Bool {
        guard let recentURL = try? MarkdownSourceLoader.resolveBookmark(recent.bookmarkData) else {
            return false
        }
        return recentURL.standardizedFileURL == url.standardizedFileURL
    }

    private static func loadRecents(from defaults: UserDefaults) -> [MarkdownRecentDocument] {
        guard let data = defaults.data(forKey: recentsStorageKey),
              let decoded = try? JSONDecoder().decode([MarkdownRecentDocument].self, from: data) else {
            return []
        }
        return Array(decoded.prefix(maximumRecentDocuments))
    }

    private static func loadActiveDocument(
        from defaults: UserDefaults
    ) -> MarkdownActiveDocumentReference? {
        guard let data = defaults.data(forKey: activeDocumentStorageKey) else { return nil }
        return try? JSONDecoder().decode(MarkdownActiveDocumentReference.self, from: data)
    }

    private static func isUnavailableFileError(_ error: Error) -> Bool {
        let cocoaError = error as NSError
        guard cocoaError.domain == NSCocoaErrorDomain else { return false }
        return [
            CocoaError.Code.fileNoSuchFile.rawValue,
            CocoaError.Code.fileReadNoSuchFile.rawValue,
            CocoaError.Code.fileReadNoPermission.rawValue,
            CocoaError.Code.fileWriteNoPermission.rawValue
        ].contains(cocoaError.code)
    }
}

enum MarkdownSourceLoader {
    static let maximumBytes = 64 * 1_024 * 1_024
    static let ioChunkBytes = 64 * 1_024
    private static let utf8ByteOrderMark = Data([0xEF, 0xBB, 0xBF])

    enum ImportError: LocalizedError, Equatable {
        case notAFileURL
        case tooLarge(maximumBytes: Int)
        case notUTF8

        var errorDescription: String? {
            switch self {
            case .notAFileURL:
                "Choose a Markdown or plain-text file from Files."
            case .tooLarge(let maximumBytes):
                "That document is larger than the supported \(maximumBytes / 1_024 / 1_024) MB limit."
            case .notUTF8:
                "That document is not valid UTF-8 text."
            }
        }
    }

    enum DocumentError: LocalizedError, Equatable {
        case noCurrentDocument
        case changedOnDisk
        case recoveryConflict
        case coordinatedRead
        case coordinatedWrite
        case savedCopyMismatch

        var errorDescription: String? {
            switch self {
            case .noCurrentDocument:
                "Choose where to save this new Markdown document."
            case .changedOnDisk:
                "This file changed in another app. Reopen it before saving, or use Save a Copy to keep your edits."
            case .recoveryConflict:
                "This recovered draft could not be safely matched to the last file. Use Save a Copy or reopen the file."
            case .coordinatedRead:
                "The document provider could not coordinate a safe read of this file."
            case .coordinatedWrite:
                "The document provider could not coordinate a safe save of this file."
            case .savedCopyMismatch:
                "The saved copy could not be verified. Your current document was not replaced."
            }
        }
    }

    static func load(from url: URL) throws -> MarkdownSourceDocument {
        guard url.isFileURL else { throw ImportError.notAFileURL }
        return try withSecurityScopedAccess(to: url) {
            let loaded = try coordinatedRead(from: url)
            return MarkdownSourceDocument(
                url: url,
                bookmarkData: try bookmark(for: url),
                source: loaded.source,
                suggestedTitle: suggestedTitle(for: url),
                diskFingerprint: loaded.diskFingerprint,
                sourceDigest: loaded.sourceDigest
            )
        }
    }

    static func open(from url: URL) async throws -> MarkdownSourceDocument {
        try Task.checkCancellation()
        let worker = Task.detached(priority: .userInitiated) {
            try load(from: url)
        }
        return try await withTaskCancellationHandler {
            try await worker.value
        } onCancel: {
            worker.cancel()
        }
    }

    static func save(
        _ source: String,
        replacing document: MarkdownSourceDocument
    ) async throws -> MarkdownSourceDocument {
        try Task.checkCancellation()
        let worker = Task.detached(priority: .userInitiated) {
            try withSecurityScopedAccess(to: document.url) {
                // Resolve bookmark metadata before committing the replacement:
                // a metadata failure must not make a successful write look lost.
                let bookmarkData = try bookmark(for: document.url)
                let written = try coordinatedReplace(
                    at: document.url,
                    expected: document.diskFingerprint,
                    source: source
                )
                return MarkdownSourceDocument(
                    url: document.url,
                    // Replacement may change the file identity. Refresh the
                    // bookmark, but never report a committed write as lost if
                    // its provider cannot immediately supply fresh metadata.
                    bookmarkData: (try? bookmark(for: document.url)) ?? bookmarkData,
                    source: source,
                    suggestedTitle: suggestedTitle(for: document.url),
                    diskFingerprint: written.diskFingerprint,
                    sourceDigest: written.sourceDigest
                )
            }
        }
        return try await withTaskCancellationHandler {
            try await worker.value
        } onCancel: {
            worker.cancel()
        }
    }

    static func resolveBookmark(_ data: Data) throws -> URL {
        var stale = false
        var options: URL.BookmarkResolutionOptions = [
            .withoutUI,
            .withoutImplicitStartAccessing
        ]
#if targetEnvironment(macCatalyst)
        options.insert(.withSecurityScope)
#endif
        return try URL(
            resolvingBookmarkData: data,
            options: options,
            relativeTo: nil,
            bookmarkDataIsStale: &stale
        )
    }

    static func decode(
        _ data: Data,
        maximumBytes: Int = maximumBytes
    ) throws -> String {
        guard data.count <= maximumBytes else {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        let bytes = data.starts(with: utf8ByteOrderMark)
            ? data.dropFirst(utf8ByteOrderMark.count) : data[...]
        return try decodeUTF8(bytes)
    }

    static func encode(_ source: String, includingByteOrderMark: Bool = false) throws -> Data {
        let byteCount = source.utf8.count + (includingByteOrderMark ? utf8ByteOrderMark.count : 0)
        guard byteCount <= maximumBytes else {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        var data = Data()
        data.reserveCapacity(byteCount)
        if includingByteOrderMark { data.append(utf8ByteOrderMark) }
        data.append(contentsOf: source.utf8)
        return data
    }

    /// Hash UTF-8 without allocating a complete encoded copy on the main actor.
    static func sourceDigest(_ source: String) throws -> Data {
        try Task.checkCancellation()
        var digest = SHA256()
        try forEachUTF8Chunk(source) { chunk in
            try Task.checkCancellation()
            digest.update(data: chunk)
        }
        return Data(digest.finalize())
    }

    private static func forEachUTF8Chunk(
        _ source: String,
        consume: (Data) throws -> Void
    ) rethrows {
        let bytes = source.utf8
        var start = bytes.startIndex
        while start != bytes.endIndex {
            let end = bytes.index(start, offsetBy: ioChunkBytes, limitedBy: bytes.endIndex) ?? bytes.endIndex
            try consume(Data(bytes[start..<end]))
            start = end
        }
    }

    struct LoadedSource: Sendable {
        let source: String
        let diskFingerprint: MarkdownSourceFingerprint
        let sourceDigest: Data
    }

    /// A strict incremental UTF-8 decoder. At most three bytes cross a read
    /// boundary, including when the provider returns short reads. The source
    /// String is the only document-sized allocation retained by the result.
    static func readSource(
        maximumBytes: Int = maximumBytes,
        capacityHint: Int = 0,
        read: (Int) throws -> Data
    ) throws -> LoadedSource {
        try Task.checkCancellation()
        guard maximumBytes >= 0 else {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        var source = ""
        source.reserveCapacity(min(maximumBytes, max(0, capacityHint)))
        var pending = Data()
        var atStart = true
        var hasByteOrderMark = false
        var diskDigest = SHA256()
        var textDigest = SHA256()
        let byteCount = try readChunks(maximumBytes: maximumBytes, read: read) { chunk in
            diskDigest.update(data: chunk)
            pending.append(chunk)
            if atStart {
                if pending.count < utf8ByteOrderMark.count,
                   utf8ByteOrderMark.starts(with: pending) { return }
                hasByteOrderMark = pending.starts(with: utf8ByteOrderMark)
                if hasByteOrderMark { pending.removeFirst(utf8ByteOrderMark.count) }
                atStart = false
            }
            guard !pending.isEmpty else { return }
            var lastLead = pending.index(before: pending.endIndex)
            while lastLead > pending.startIndex, pending[lastLead] & 0xC0 == 0x80 {
                lastLead = pending.index(before: lastLead)
            }
            let lead = pending[lastLead]
            let expectedLength: Int
            switch lead {
            case 0xC2...0xDF: expectedLength = 2
            case 0xE0...0xEF: expectedLength = 3
            case 0xF0...0xF4: expectedLength = 4
            default: expectedLength = 1
            }
            let end = pending.distance(from: lastLead, to: pending.endIndex) < expectedLength
                ? lastLead : pending.endIndex
            let complete = pending[..<end]
            source.append(try decodeUTF8(complete))
            textDigest.update(data: complete)
            pending = Data(pending[end...])
        }
        // A partial scalar or partial BOM is invalid at EOF. A complete BOM
        // was consumed above, even when it arrived one byte at a time.
        guard pending.isEmpty else { throw ImportError.notUTF8 }
        return LoadedSource(
            source: source,
            diskFingerprint: MarkdownSourceFingerprint(
                byteCount: byteCount,
                digest: Data(diskDigest.finalize()),
                hasByteOrderMark: hasByteOrderMark
            ),
            sourceDigest: Data(textDigest.finalize())
        )
    }

    private static func decodeUTF8(_ bytes: Data) throws -> String {
        // Foundation's external-representation decoder may interpret a BOM
        // at each chunk boundary. The native decoder preserves all scalars;
        // byte equality rejects any replacement it made for malformed UTF-8.
        // String(validating:as:) is unavailable on the Swift 5.10/iOS 17 floor.
        let decoded = String(decoding: bytes, as: UTF8.self)
        guard decoded.utf8.elementsEqual(bytes) else { throw ImportError.notUTF8 }
        return decoded
    }

    /// Continue after short reads and probe one byte past the limit. Reading
    /// once with a large count is not an EOF guarantee for document providers.
    private static func readChunks(
        maximumBytes: Int = maximumBytes,
        read: (Int) throws -> Data,
        consume: (Data) throws -> Void
    ) throws -> Int {
        var byteCount = 0
        while true {
            try Task.checkCancellation()
            let remaining = maximumBytes - byteCount
            let chunk = try read(remaining == 0 ? 1 : min(ioChunkBytes, remaining))
            guard !chunk.isEmpty else { return byteCount }
            guard chunk.count <= maximumBytes - byteCount else {
                throw ImportError.tooLarge(maximumBytes: maximumBytes)
            }
            byteCount += chunk.count
            try consume(chunk)
        }
    }

    private static func suggestedTitle(for url: URL) -> String {
        url.deletingPathExtension().lastPathComponent
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func bookmark(for url: URL) throws -> Data {
#if targetEnvironment(macCatalyst)
        let options: URL.BookmarkCreationOptions = [.withSecurityScope]
#else
        let options: URL.BookmarkCreationOptions = []
#endif
        return try url.bookmarkData(
            options: options,
            includingResourceValuesForKeys: nil,
            relativeTo: nil
        )
    }

    private static func coordinatedRead(from url: URL) throws -> LoadedSource {
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<LoadedSource, Error>?
        coordinator.coordinate(readingItemAt: url, options: [], error: &coordinationError) { coordinatedURL in
            result = Result {
                let size = try checkReadableFile(at: coordinatedURL)
                let handle = try FileHandle(forReadingFrom: coordinatedURL)
                defer { try? handle.close() }
                return try readSource(capacityHint: size) { count in
                    try handle.read(upToCount: count) ?? Data()
                }
            }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw DocumentError.coordinatedRead }
        return try result.get()
    }

    private static func coordinatedReplace(
        at url: URL,
        expected: MarkdownSourceFingerprint,
        source: String
    ) throws -> LoadedSource {
        let byteCount = source.utf8.count + (expected.hasByteOrderMark ? utf8ByteOrderMark.count : 0)
        guard byteCount <= maximumBytes else {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<LoadedSource, Error>?
        coordinator.coordinate(
            writingItemAt: url,
            options: .forReplacing,
            error: &coordinationError
        ) { coordinatedURL in
            result = Result {
                let current = try fingerprint(at: coordinatedURL)
                guard current.byteCount == expected.byteCount, current.digest == expected.digest else {
                    throw DocumentError.changedOnDisk
                }
                return try writeAtomically(source, to: coordinatedURL, includingByteOrderMark: expected.hasByteOrderMark)
            }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw DocumentError.coordinatedWrite }
        return try result.get()
    }

    private static func fingerprint(at url: URL) throws -> (byteCount: Int, digest: Data) {
        _ = try checkReadableFile(at: url)
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var digest = SHA256()
        let byteCount = try readChunks(read: { count in
            try handle.read(upToCount: count) ?? Data()
        }) { digest.update(data: $0) }
        return (byteCount: byteCount, digest: Data(digest.finalize()))
    }

    private static func writeAtomically(
        _ source: String,
        to url: URL,
        includingByteOrderMark: Bool
    ) throws -> LoadedSource {
        let manager = FileManager.default
        let directory = try manager.url(
            for: .itemReplacementDirectory,
            in: .userDomainMask,
            appropriateFor: url,
            create: true
        )
        defer { try? manager.removeItem(at: directory) }
        let replacement = directory.appendingPathComponent("source.md", isDirectory: false)
        guard manager.createFile(atPath: replacement.path, contents: nil) else {
            throw CocoaError(.fileWriteUnknown)
        }
        let handle = try FileHandle(forWritingTo: replacement)
        defer { try? handle.close() }
        var diskDigest = SHA256()
        var textDigest = SHA256()
        var byteCount = 0
        if includingByteOrderMark {
            try handle.write(contentsOf: utf8ByteOrderMark)
            diskDigest.update(data: utf8ByteOrderMark)
            byteCount += utf8ByteOrderMark.count
        }
        try forEachUTF8Chunk(source) { chunk in
            try Task.checkCancellation()
            try handle.write(contentsOf: chunk)
            diskDigest.update(data: chunk)
            textDigest.update(data: chunk)
            byteCount += chunk.count
        }
        try handle.synchronize()
        try handle.close()
        try Task.checkCancellation()
        _ = try manager.replaceItemAt(url, withItemAt: replacement)
        return LoadedSource(
            source: source,
            diskFingerprint: MarkdownSourceFingerprint(
                byteCount: byteCount,
                digest: Data(diskDigest.finalize()),
                hasByteOrderMark: includingByteOrderMark
            ),
            sourceDigest: Data(textDigest.finalize())
        )
    }

    private static func checkReadableFile(at url: URL) throws -> Int {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile != false else { throw ImportError.notAFileURL }
        if let size = values.fileSize, size > maximumBytes {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        return values.fileSize ?? 0
    }

    private static func withSecurityScopedAccess<T>(to url: URL, body: () throws -> T) throws -> T {
        guard url.isFileURL else { throw ImportError.notAFileURL }
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        return try body()
    }
}
