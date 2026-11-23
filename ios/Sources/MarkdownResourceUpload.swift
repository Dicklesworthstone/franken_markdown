import Foundation

struct MarkdownResourceUploadStart: Decodable, Equatable, Sendable {
    let resourceSetID: Int
    let entryCount: Int
    let byteLength: Int
    let chunkSize: Int
}

struct MarkdownResourceUploadManifest: Sendable {
    struct Entry: Sendable {
        let kind: String
        let key: String
        let byteLength: Int
    }
    let start: MarkdownResourceUploadStart
    let entries: [Entry]

    var arguments: [String: Any] {
        ["resourceSetID": start.resourceSetID, "byteLength": start.byteLength,
         "chunkSize": start.chunkSize, "entries": entries.map {
            ["kind": $0.kind, "key": $0.key, "byteLength": $0.byteLength] as [String: Any]
         }]
    }
}

struct MarkdownResourceUploadChunk: Sendable {
    let resourceSetID: Int
    let assetIndex: Int
    let index: Int
    let byteLength: Int
    let base64: String
    let checksum: String
    let receivedBytes: Int

    var arguments: [String: Any] {
        ["resourceSetID": resourceSetID, "assetIndex": assetIndex, "index": index,
         "byteLength": byteLength, "base64": base64, "checksum": checksum]
    }
}

struct MarkdownResourceUploadAcknowledgement: Decodable, Equatable, Sendable {
    let resourceSetID: Int
    let assetIndex: Int
    let index: Int
    let receivedBytes: Int
}

struct MarkdownResourceUploadReceipt: Decodable, Equatable, Sendable {
    let resourceSetID: Int
    let byteLength: Int
    let chunkCount: Int
    let entryCount: Int

    var arguments: [String: Any] {
        ["resourceSetID": resourceSetID, "byteLength": byteLength,
         "chunkCount": chunkCount, "entryCount": entryCount]
    }
}

/// Shares the native Data snapshots and encodes only one acknowledged chunk
/// at a time. No whole-attachment base64 copy crosses the WebKit boundary.
actor MarkdownResourceUpload {
    static let chunkSize = 64 * 1_024
    private var entries: [MarkdownResourceAttachment]
    private let resourceSetID: Int
    private var start: MarkdownResourceUploadStart?
    private var assetIndex = 0
    private var assetOffset = 0
    private var nextIndex = 0
    private var sentBytes = 0
    private var isClosed = false

    init(resources: MarkdownResourceCollection, resourceSetID: Int) {
        entries = resources.entries
        self.resourceSetID = resourceSetID
    }

    func manifest() throws -> MarkdownResourceUploadManifest {
        try Task.checkCancellation()
        guard !isClosed else { throw MarkdownSourceUpload.TransferError.closed }
        guard resourceSetID > 0, resourceSetID <= 9_007_199_254_740_991 else {
            throw MarkdownSourceUpload.TransferError.invalidManifest
        }
        let collection = try MarkdownResourceCollection(validating: entries)
        let header = MarkdownResourceUploadStart(
            resourceSetID: resourceSetID, entryCount: entries.count,
            byteLength: collection.byteCount, chunkSize: Self.chunkSize
        )
        start = header
        return MarkdownResourceUploadManifest(start: header, entries: entries.map {
            .init(kind: $0.kind.rawValue, key: $0.key, byteLength: $0.bytes.count)
        })
    }

    func nextChunk() throws -> MarkdownResourceUploadChunk? {
        try Task.checkCancellation()
        guard !isClosed else { throw MarkdownSourceUpload.TransferError.closed }
        guard start != nil else { throw MarkdownSourceUpload.TransferError.notStarted }
        guard assetIndex < entries.count else { return nil }
        let bytes = entries[assetIndex].bytes
        let end = min(bytes.count, assetOffset + Self.chunkSize)
        let chunk = bytes.subdata(in: (bytes.startIndex + assetOffset)..<(bytes.startIndex + end))
        let output = MarkdownResourceUploadChunk(
            resourceSetID: resourceSetID, assetIndex: assetIndex, index: nextIndex,
            byteLength: chunk.count, base64: chunk.base64EncodedString(),
            checksum: MarkdownExportWriter.checksum(chunk), receivedBytes: sentBytes + chunk.count
        )
        assetOffset = end
        if assetOffset == bytes.count { assetIndex += 1; assetOffset = 0 }
        sentBytes += chunk.count
        nextIndex += 1
        return output
    }

    func finish() throws -> MarkdownResourceUploadReceipt {
        try Task.checkCancellation()
        guard !isClosed else { throw MarkdownSourceUpload.TransferError.closed }
        guard let start else { throw MarkdownSourceUpload.TransferError.notStarted }
        guard sentBytes == start.byteLength, assetIndex == start.entryCount else {
            throw MarkdownSourceUpload.TransferError.incomplete
        }
        let receipt = MarkdownResourceUploadReceipt(
            resourceSetID: resourceSetID, byteLength: sentBytes,
            chunkCount: nextIndex, entryCount: start.entryCount
        )
        abort()
        return receipt
    }

    func abort() { isClosed = true; entries = [] }
}
