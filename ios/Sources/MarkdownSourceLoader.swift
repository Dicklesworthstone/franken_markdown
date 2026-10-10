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
    let diskData: Data

    var displayName: String { url.lastPathComponent }
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

            let recoveredSourceDigest = Self.digest(Data(recoveredSource.utf8))
            if recoveredSourceDigest == reference.baselineSourceDigest {
                return .fileVersion(restored)
            }
            if Self.digest(document.diskData) == reference.baselineDiskDigest {
                return .recoveredEdits(restored)
            }
            return .conflict(restored)
        } catch {
            guard request == requestGeneration, isCurrentSource() else { return .none }
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
            baselineSourceDigest: Self.digest(Data(document.source.utf8)),
            baselineDiskDigest: Self.digest(document.diskData)
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

    private static func digest(_ data: Data) -> Data {
        Data(SHA256.hash(data: data))
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
            let data = try coordinatedRead(from: url)
            return try makeDocument(url: url, data: data)
        }
    }

    static func open(from url: URL) async throws -> MarkdownSourceDocument {
        try await Task.detached(priority: .userInitiated) {
            try load(from: url)
        }.value
    }

    static func save(
        _ source: String,
        replacing document: MarkdownSourceDocument
    ) async throws -> MarkdownSourceDocument {
        try await Task.detached(priority: .userInitiated) {
            try withSecurityScopedAccess(to: document.url) {
                let data = try encode(
                    source,
                    includingByteOrderMark: document.diskData.starts(with: utf8ByteOrderMark)
                )
                try coordinatedReplace(
                    at: document.url,
                    expectedData: document.diskData,
                    replacementData: data
                )
                return MarkdownSourceDocument(
                    url: document.url,
                    bookmarkData: try bookmark(for: document.url),
                    source: source,
                    suggestedTitle: suggestedTitle(for: document.url),
                    diskData: data
                )
            }
        }.value
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
        var bytes = data
        if bytes.starts(with: utf8ByteOrderMark) { bytes.removeFirst(utf8ByteOrderMark.count) }
        guard let source = String(data: bytes, encoding: .utf8) else {
            throw ImportError.notUTF8
        }
        return source
    }

    static func encode(_ source: String, includingByteOrderMark: Bool = false) throws -> Data {
        var data = Data(source.utf8)
        if includingByteOrderMark { data.insert(contentsOf: utf8ByteOrderMark, at: 0) }
        guard data.count <= maximumBytes else {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        return data
    }

    private static func makeDocument(url: URL, data: Data) throws -> MarkdownSourceDocument {
        MarkdownSourceDocument(
            url: url,
            bookmarkData: try bookmark(for: url),
            source: try decode(data),
            suggestedTitle: suggestedTitle(for: url),
            diskData: data
        )
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

    private static func coordinatedRead(from url: URL) throws -> Data {
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<Data, Error>?
        coordinator.coordinate(readingItemAt: url, options: [], error: &coordinationError) { coordinatedURL in
            result = Result { try boundedRead(from: coordinatedURL) }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw DocumentError.coordinatedRead }
        return try result.get()
    }

    private static func coordinatedReplace(
        at url: URL,
        expectedData: Data,
        replacementData: Data
    ) throws {
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<Void, Error>?
        coordinator.coordinate(
            writingItemAt: url,
            options: .forReplacing,
            error: &coordinationError
        ) { coordinatedURL in
            result = Result {
                let currentData = try boundedRead(from: coordinatedURL)
                guard currentData == expectedData else { throw DocumentError.changedOnDisk }
                try replacementData.write(to: coordinatedURL, options: .atomic)
            }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw DocumentError.coordinatedWrite }
        try result.get()
    }

    private static func boundedRead(from url: URL) throws -> Data {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile != false else { throw ImportError.notAFileURL }
        if let size = values.fileSize, size > maximumBytes {
            throw ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        return try handle.read(upToCount: maximumBytes + 1) ?? Data()
    }

    private static func withSecurityScopedAccess<T>(to url: URL, body: () throws -> T) throws -> T {
        guard url.isFileURL else { throw ImportError.notAFileURL }
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        return try body()
    }
}
