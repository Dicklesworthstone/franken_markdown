import CoreTransferable
import CryptoKit
import Foundation
import UniformTypeIdentifiers

/// Owns an immutable source snapshot for the system Files save dialog. The
/// dialog receives a file representation rather than a document-sized Data or
/// FileWrapper. ARC keeps this private file alive through transfer completion.
final class MarkdownSourceExport: Transferable, Sendable {
    let fileURL: URL
    let byteCount: Int
    let sourceDigest: Data
    private let directory: URL

    private init(fileURL: URL, directory: URL, byteCount: Int, sourceDigest: Data) {
        self.fileURL = fileURL
        self.directory = directory
        self.byteCount = byteCount
        self.sourceDigest = sourceDigest
    }

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .frankenMarkdownSource) { snapshot in
            SentTransferredFile(snapshot.fileURL)
        }
    }

    static func prepare(
        source: String,
        includingByteOrderMark: Bool,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory
    ) async throws -> MarkdownSourceExport {
        try Task.checkCancellation()
        let worker = Task.detached(priority: .userInitiated) {
            try prepareSynchronously(
                source: source, includingByteOrderMark: includingByteOrderMark,
                temporaryDirectory: temporaryDirectory
            )
        }
        return try await withTaskCancellationHandler {
            let snapshot = try await worker.value
            try Task.checkCancellation()
            return snapshot
        } onCancel: {
            worker.cancel()
        }
    }

    static func prepareSynchronously(
        source: String,
        includingByteOrderMark: Bool,
        temporaryDirectory: URL
    ) throws -> MarkdownSourceExport {
        try Task.checkCancellation()
        // A literal leading U+FEFF needs a separate encoding marker, otherwise
        // reopening a new file would interpret that source character as a BOM.
        let hasBOM = includingByteOrderMark || source.unicodeScalars.first?.value == 0xFEFF
        try validateSize(source, includingByteOrderMark: hasBOM, maximumBytes: MarkdownSourceLoader.maximumBytes)
        let manager = FileManager.default
        let directory = temporaryDirectory.appendingPathComponent(
            "FrankenMarkdown-Source-\(UUID().uuidString)", isDirectory: true
        )
        try manager.createDirectory(
            at: directory, withIntermediateDirectories: false,
            attributes: [.protectionKey: FileProtectionType.complete]
        )
        do {
            var protectedDirectory = directory
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try protectedDirectory.setResourceValues(values)
            let fileURL = directory.appendingPathComponent("source.md", isDirectory: false)
            guard manager.createFile(
                atPath: fileURL.path, contents: nil,
                attributes: [.protectionKey: FileProtectionType.complete]
            ) else { throw CocoaError(.fileWriteUnknown) }
            let handle = try FileHandle(forWritingTo: fileURL)
            defer { try? handle.close() }
            let written = try writeSource(source, includingByteOrderMark: hasBOM) {
                try handle.write(contentsOf: $0)
            }
            try handle.synchronize()
            try handle.close()
            try Task.checkCancellation()
            return MarkdownSourceExport(
                fileURL: fileURL, directory: directory,
                byteCount: written.byteCount, sourceDigest: written.sourceDigest
            )
        } catch {
            // Only this request's newly created private directory is removed.
            try? manager.removeItem(at: directory)
            throw error
        }
    }

    /// The sink boundary lets tests verify bounded writes and failure handling
    /// without constructing another complete encoded source.
    static func writeSource(
        _ source: String,
        includingByteOrderMark: Bool,
        maximumBytes: Int = MarkdownSourceLoader.maximumBytes,
        consume: (Data) throws -> Void
    ) throws -> (byteCount: Int, sourceDigest: Data) {
        try Task.checkCancellation()
        let count = try validateSize(source, includingByteOrderMark: includingByteOrderMark, maximumBytes: maximumBytes)
        if includingByteOrderMark { try consume(Data([0xEF, 0xBB, 0xBF])) }
        var digest = SHA256()
        try MarkdownSourceLoader.forEachUTF8Chunk(source) { chunk in
            try Task.checkCancellation()
            try consume(chunk)
            digest.update(data: chunk)
        }
        return (count, Data(digest.finalize()))
    }

    @discardableResult
    private static func validateSize(
        _ source: String, includingByteOrderMark: Bool, maximumBytes: Int
    ) throws -> Int {
        let prefixBytes = includingByteOrderMark ? 3 : 0
        let count = source.utf8.count
        guard maximumBytes >= prefixBytes, count <= maximumBytes - prefixBytes else {
            throw MarkdownSourceLoader.ImportError.tooLarge(maximumBytes: maximumBytes)
        }
        return count + prefixBytes
    }

    deinit { try? FileManager.default.removeItem(at: directory) }
}
