import Foundation

/// Only this small manifest, followed by one requested chunk at a time, crosses
/// the WebKit boundary. The renderer's complete output stays in its process.
struct MarkdownExportManifest: Decodable, Sendable {
    let requestID: Int
    let byteLength: Int
    let `extension`: String
    let mimeType: String
    let diagnosticCount: Int
    let chunkSize: Int
}

struct MarkdownExportChunk: Decodable, Sendable {
    let requestID: Int
    let index: Int
    let base64: String
    let byteLength: Int
    let checksum: String
}

struct RenderedArtifact: Sendable {
    let fileURL: URL
    let byteLength: Int
    let `extension`: String
    let mimeType: String
    let diagnosticCount: Int
}

/// A single-use, off-main-actor sink. Never assembles an exported document in
/// Data or String, and never exposes a partial file to a share sheet.
actor MarkdownExportWriter {
    static let chunkSize = 64 * 1_024
    static let maximumBytes = 512 * 1_024 * 1_024

    enum TransferError: Error, LocalizedError, Equatable {
        case invalidManifest
        case invalidChunk
        case incomplete
        case closed
        case cannotCreateFile

        var errorDescription: String? {
            switch self {
            case .invalidManifest: "The renderer returned an invalid or oversized export."
            case .invalidChunk: "The exported file failed its chunk integrity check."
            case .incomplete: "The renderer did not transfer the complete exported file."
            case .closed: "This export transfer has already ended."
            case .cannotCreateFile: "The exported file could not be created in temporary storage."
            }
        }
    }

    private let manifest: MarkdownExportManifest
    private let directory: URL
    private let partialURL: URL
    private let finalURL: URL
    private var handle: FileHandle?
    private var receivedBytes = 0
    private var nextIndex = 0
    private var finished = false

    init(
        manifest: MarkdownExportManifest,
        requestID: Int,
        title: String,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory
    ) throws {
        let formats = [
            "pdf": "application/pdf", "html": "text/html", "svg": "image/svg+xml",
            "epub": "application/epub+zip", "json": "application/json", "zip": "application/zip"
        ]
        guard manifest.requestID == requestID, requestID > 0,
              (0...Self.maximumBytes).contains(manifest.byteLength),
              manifest.chunkSize == Self.chunkSize, manifest.diagnosticCount >= 0,
              let expectedType = formats[manifest.extension],
              manifest.mimeType.split(separator: ";", maxSplits: 1).first.map(String.init) == expectedType else {
            throw TransferError.invalidManifest
        }
        self.manifest = manifest
        directory = temporaryDirectory.appendingPathComponent(
            "FrankenMarkdown-Export-\(UUID().uuidString)", isDirectory: true
        )
        let trimmed = String(decoding: title.utf8.prefix(120), as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        let forbidden = CharacterSet(charactersIn: "/:\\?%*|\"<>").union(.controlCharacters)
        let safe = trimmed.components(separatedBy: forbidden).joined(separator: "-")
        let name = safe.isEmpty || safe == "." || safe == ".." ? "Document" : safe
        finalURL = directory.appendingPathComponent("\(name).\(manifest.extension)")
        partialURL = directory.appendingPathComponent("transfer.partial")
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: false,
            attributes: [.protectionKey: FileProtectionType.complete]
        )
        do {
            guard FileManager.default.createFile(
                atPath: partialURL.path, contents: nil,
                attributes: [.protectionKey: FileProtectionType.complete]
            ) else { throw TransferError.cannotCreateFile }
            handle = try FileHandle(forWritingTo: partialURL)
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
    }

    func append(_ chunk: MarkdownExportChunk) throws {
        try Task.checkCancellation()
        guard let handle, !finished else { throw TransferError.closed }
        let expectedBytes = min(Self.chunkSize, manifest.byteLength - receivedBytes)
        // Check encoded size before decoding, so malformed messages cannot ask
        // Foundation to allocate an unbounded second buffer.
        guard expectedBytes > 0, chunk.requestID == manifest.requestID,
              chunk.index == nextIndex, chunk.byteLength == expectedBytes,
              chunk.base64.utf8.count == ((expectedBytes + 2) / 3) * 4,
              chunk.checksum.utf8.count == 8,
              let bytes = Data(base64Encoded: chunk.base64), bytes.count == expectedBytes,
              Self.checksum(bytes) == chunk.checksum else {
            throw TransferError.invalidChunk
        }
        try handle.write(contentsOf: bytes)
        receivedBytes += bytes.count
        nextIndex += 1
    }

    func finish() throws -> RenderedArtifact {
        try Task.checkCancellation()
        guard let handle, !finished else { throw TransferError.closed }
        guard receivedBytes == manifest.byteLength else { throw TransferError.incomplete }
        try handle.synchronize()
        try handle.close()
        self.handle = nil
        try FileManager.default.moveItem(at: partialURL, to: finalURL)
        finished = true
        return RenderedArtifact(
            fileURL: finalURL, byteLength: receivedBytes, extension: manifest.extension,
            mimeType: manifest.mimeType, diagnosticCount: manifest.diagnosticCount
        )
    }

    /// Only removes this transfer's unique, app-created temporary directory.
    /// Called on failure, including cancellation just after the final rename.
    func abort() {
        try? handle?.close()
        handle = nil
        finished = true
        try? FileManager.default.removeItem(at: directory)
    }

    deinit { try? handle?.close() }

    private static let crcTable: [UInt32] = (0..<256).map { index in
        var value = UInt32(index)
        for _ in 0..<8 {
            value = (value >> 1) ^ ((value & 1) == 0 ? 0 : 0xEDB88320)
        }
        return value
    }

    /// CRC-32/ISO-HDLC detects transport corruption; it is not authentication.
    static func checksum(_ bytes: Data) -> String {
        var crc = UInt32.max
        for byte in bytes { crc = (crc >> 8) ^ crcTable[Int((crc ^ UInt32(byte)) & 0xff)] }
        return String(format: "%08x", crc ^ UInt32.max)
    }
}
