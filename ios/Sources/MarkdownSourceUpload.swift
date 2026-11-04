import Foundation

struct MarkdownSourceUploadManifest: Decodable, Equatable, Sendable {
    let sourceID: Int
    let byteLength: Int
    let chunkSize: Int

    var arguments: [String: Any] {
        ["sourceID": sourceID, "byteLength": byteLength, "chunkSize": chunkSize]
    }
}

struct MarkdownSourceUploadChunk: Sendable {
    let sourceID: Int
    let index: Int
    let byteLength: Int
    let base64: String
    let checksum: String
    let receivedBytes: Int

    var arguments: [String: Any] {
        ["sourceID": sourceID, "index": index, "byteLength": byteLength,
         "base64": base64, "checksum": checksum]
    }
}

struct MarkdownSourceUploadAcknowledgement: Decodable, Sendable {
    let sourceID: Int
    let index: Int
    let receivedBytes: Int
}

struct MarkdownSourceUploadReceipt: Decodable, Equatable, Sendable {
    let sourceID: Int
    let byteLength: Int
    let chunkCount: Int

    var arguments: [String: Any] {
        ["sourceID": sourceID, "byteLength": byteLength, "chunkCount": chunkCount]
    }
}

/// Traverses one immutable source snapshot off the main actor. Only the next
/// UTF-8 chunk is copied and encoded; a complete Data/base64 copy is never made.
/// Chunks may split a Unicode scalar: the receiver uses a streaming decoder.
actor MarkdownSourceUpload {
    static let chunkSize = 64 * 1_024
    static let maximumBytes = MarkdownSourceLoader.maximumBytes

    enum TransferError: Error, LocalizedError, Equatable {
        case invalidManifest
        case notStarted
        case incomplete
        case closed
        case invalidAcknowledgement

        var errorDescription: String? {
            switch self {
            case .invalidManifest: "That source is larger than the supported 64 MB limit or has an invalid transfer identifier."
            case .notStarted: "The source transfer has not started."
            case .incomplete: "The source transfer did not include the complete document."
            case .closed: "This source transfer has already ended."
            case .invalidAcknowledgement: "The source transfer was not acknowledged correctly."
            }
        }
    }

    private var source: String
    private let sourceID: Int
    private let maximumByteCount: Int
    private var manifestValue: MarkdownSourceUploadManifest?
    private var cursor: String.UTF8View.Index?
    private var sentBytes = 0
    private var nextIndex = 0
    private var isClosed = false

    init(
        source: String,
        sourceID: Int,
        maximumBytes: Int = MarkdownSourceUpload.maximumBytes
    ) {
        self.source = source
        self.sourceID = sourceID
        maximumByteCount = maximumBytes
    }

    func manifest() throws -> MarkdownSourceUploadManifest {
        try Task.checkCancellation()
        guard !isClosed else { throw TransferError.closed }
        if let manifestValue { return manifestValue }
        guard sourceID > 0, sourceID <= 9_007_199_254_740_991,
              maximumByteCount >= 0 else { throw TransferError.invalidManifest }
        let utf8 = source.utf8
        let byteLength = utf8.count
        guard byteLength <= maximumByteCount else { throw TransferError.invalidManifest }
        let value = MarkdownSourceUploadManifest(
            sourceID: sourceID, byteLength: byteLength, chunkSize: Self.chunkSize
        )
        manifestValue = value
        cursor = utf8.startIndex
        return value
    }

    func nextChunk() throws -> MarkdownSourceUploadChunk? {
        try Task.checkCancellation()
        guard !isClosed else { throw TransferError.closed }
        guard manifestValue != nil, let start = cursor else { throw TransferError.notStarted }
        let utf8 = source.utf8
        guard start != utf8.endIndex else { return nil }
        let end = utf8.index(start, offsetBy: Self.chunkSize, limitedBy: utf8.endIndex) ?? utf8.endIndex
        let bytes = Data(utf8[start..<end])
        let chunk = MarkdownSourceUploadChunk(
            sourceID: sourceID, index: nextIndex, byteLength: bytes.count,
            base64: bytes.base64EncodedString(),
            checksum: MarkdownExportWriter.checksum(bytes),
            receivedBytes: sentBytes + bytes.count
        )
        cursor = end
        sentBytes += bytes.count
        nextIndex += 1
        return chunk
    }

    func finish() throws -> MarkdownSourceUploadReceipt {
        try Task.checkCancellation()
        guard !isClosed else { throw TransferError.closed }
        guard let manifestValue else { throw TransferError.notStarted }
        guard sentBytes == manifestValue.byteLength else { throw TransferError.incomplete }
        let receipt = MarkdownSourceUploadReceipt(
            sourceID: sourceID, byteLength: sentBytes, chunkCount: nextIndex
        )
        abort()
        return receipt
    }

    func abort() {
        isClosed = true
        source = ""
        cursor = nil
    }
}
