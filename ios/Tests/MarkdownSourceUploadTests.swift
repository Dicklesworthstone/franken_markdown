import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownSourceUploadTests: XCTestCase {
    @MainActor
    func testModelInvalidatesHandlesForCanonicallyEquivalentSourceAssignments() {
        let model = MarkdownRendererModel()
        let initialRevision = model.sourceRevision
        model.source = "caf\u{00E9}"
        let firstRevision = model.sourceRevision
        let firstBytes = Data(model.source.utf8)
        model.source = "cafe\u{0301}"
        XCTAssertEqual("caf\u{00E9}", "cafe\u{0301}")
        XCTAssertNotEqual(firstBytes, Data(model.source.utf8))
        XCTAssertEqual(firstRevision, initialRevision + 1)
        XCTAssertEqual(model.sourceRevision, firstRevision + 1)
        // A repeated assignment remains constant-time and owns a new revision.
        model.source = "cafe\u{0301}"
        XCTAssertEqual(model.sourceRevision, firstRevision + 2)
    }

    func testCanonicallyEquivalentUnicodeSnapshotsRetainDistinctUTF8Bytes() async throws {
        let composed = "caf\u{00E9}"
        let decomposed = "cafe\u{0301}"
        XCTAssertEqual(composed, decomposed)
        let first = MarkdownSourceUpload(source: composed, sourceID: 1)
        let second = MarkdownSourceUpload(source: decomposed, sourceID: 2)
        let firstManifest = try await first.manifest()
        let secondManifest = try await second.manifest()
        XCTAssertNotEqual(firstManifest.byteLength, secondManifest.byteLength)
        let firstPayload = try await collect(first)
        let secondPayload = try await collect(second)
        XCTAssertEqual(firstPayload.bytes, Data(composed.utf8))
        XCTAssertEqual(secondPayload.bytes, Data(decomposed.utf8))
        XCTAssertNotEqual(firstPayload.bytes, secondPayload.bytes)
        _ = try await first.finish()
        _ = try await second.finish()
    }

    func testEmptyAndBoundarySizedSourcesHaveExactBytesAndReceipts() async throws {
        for size in [0, 1, 65_535, 65_536, 65_537, 196_615] {
            let source = String(repeating: "x", count: size)
            let upload = MarkdownSourceUpload(source: source, sourceID: 7)
            let manifest = try await upload.manifest()
            XCTAssertEqual(manifest.sourceID, 7)
            XCTAssertEqual(manifest.byteLength, size)
            XCTAssertEqual(manifest.chunkSize, 65_536)
            let secondManifest = try await upload.manifest()
            XCTAssertEqual(secondManifest, manifest)
            let collected = try await collect(upload)
            XCTAssertEqual(collected.bytes, Data(source.utf8))
            let receipt = try await upload.finish()
            XCTAssertEqual(receipt, MarkdownSourceUploadReceipt(
                sourceID: 7, byteLength: size, chunkCount: collected.chunkCount
            ))
            await expectFailure(.closed) { _ = try await upload.nextChunk() }
            await expectFailure(.closed) { _ = try await upload.finish() }
            await expectFailure(.closed) { _ = try await upload.manifest() }
        }
    }

    func testEveryUTF8ScalarSplitPreservesBOMCombiningMarksAndLineEndings() async throws {
        for scalar in ["é", "漢", "🧪", "\u{FEFF}"] {
            for split in 1..<scalar.utf8.count {
                let prefix = "\u{FEFF}" + String(repeating: "a", count: 65_533 - split)
                let source = prefix + scalar + "e\u{0301}\0\r\nEND\u{FEFF}"
                let upload = MarkdownSourceUpload(source: source, sourceID: 8)
                let manifest = try await upload.manifest()
                XCTAssertEqual(manifest.byteLength, source.utf8.count)
                let collected = try await collect(upload)
                XCTAssertEqual(collected.chunkCount, 2)
                XCTAssertEqual(collected.bytes, Data(source.utf8))
                XCTAssertEqual(String(data: collected.bytes, encoding: .utf8), source)
                _ = try await upload.finish()
            }
        }
    }

    func testSnapshotIsUnchangedWhenTheLiveDocumentChangesDuringUpload() async throws {
        var live = String(repeating: "a", count: 65_536) + "original 🧪 ending"
        let expected = Data(live.utf8)
        let upload = MarkdownSourceUpload(source: live, sourceID: 9)
        _ = try await upload.manifest()
        let firstValue = try await upload.nextChunk()
        let first = try XCTUnwrap(firstValue)
        live.replaceSubrange(live.startIndex..<live.endIndex, with: "A different document")
        var received = try XCTUnwrap(Data(base64Encoded: first.base64))
        let rest = try await collect(upload, startingIndex: 1, startingBytes: received.count)
        received.append(rest.bytes)
        XCTAssertEqual(received, expected)
        XCTAssertEqual(live, "A different document")
        let receipt = try await upload.finish()
        XCTAssertEqual(receipt.byteLength, expected.count)
        XCTAssertEqual(receipt.chunkCount, 2)
    }

    func testLimitsApplyToUTF8BytesBeforeAnyChunkIsPrepared() async {
        for (source, identifier, maximum) in [
            ("x", 0, 100), ("x", -1, 100),
            ("x", 9_007_199_254_740_992, 100),
            ("", 1, -1), ("éé", 1, 3), ("🧪", 1, 3)
        ] {
            let upload = MarkdownSourceUpload(source: source, sourceID: identifier, maximumBytes: maximum)
            await expectFailure(.invalidManifest) { _ = try await upload.manifest() }
            await expectFailure(.notStarted) { _ = try await upload.nextChunk() }
        }
    }

    func testExactlyAtTheByteLimitIsAllowed() async throws {
        let upload = MarkdownSourceUpload(source: "🧪", sourceID: 1, maximumBytes: 4)
        let manifest = try await upload.manifest()
        XCTAssertEqual(manifest.byteLength, 4)
        let collected = try await collect(upload)
        XCTAssertEqual(collected.bytes, Data("🧪".utf8))
        _ = try await upload.finish()
    }

    func testIncompleteAndAbortedTransfersCannotPublish() async throws {
        let upload = MarkdownSourceUpload(source: String(repeating: "x", count: 65_537), sourceID: 1)
        await expectFailure(.notStarted) { _ = try await upload.nextChunk() }
        await expectFailure(.notStarted) { _ = try await upload.finish() }
        _ = try await upload.manifest()
        _ = try await upload.nextChunk()
        await expectFailure(.incomplete) { _ = try await upload.finish() }
        await upload.abort()
        await upload.abort()
        await expectFailure(.closed) { _ = try await upload.nextChunk() }
        await expectFailure(.closed) { _ = try await upload.finish() }
    }

    func testCancellationStopsPreparationBeforeTheNextChunk() async throws {
        let upload = MarkdownSourceUpload(source: String(repeating: "x", count: 65_537), sourceID: 1)
        _ = try await upload.manifest()
        let operation = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return try await upload.nextChunk()
        }
        do {
            _ = try await operation.value
            XCTFail("Cancelled upload should not prepare another chunk")
        } catch is CancellationError {
            // Cancellation was observed before the transfer cursor advanced.
        } catch {
            XCTFail("Expected CancellationError, received \(error)")
        }
        let collected = try await collect(upload)
        XCTAssertEqual(collected.bytes.count, 65_537)
        _ = try await upload.finish()
    }

    private func collect(
        _ upload: MarkdownSourceUpload,
        startingIndex: Int = 0,
        startingBytes: Int = 0
    ) async throws -> (bytes: Data, chunkCount: Int) {
        var bytes = Data()
        var index = startingIndex
        while let chunk = try await upload.nextChunk() {
            XCTAssertEqual(chunk.index, index)
            XCTAssertGreaterThan(chunk.byteLength, 0)
            XCTAssertLessThanOrEqual(chunk.byteLength, 65_536)
            XCTAssertLessThanOrEqual(chunk.base64.utf8.count, 87_384)
            let decoded = try XCTUnwrap(Data(base64Encoded: chunk.base64))
            XCTAssertEqual(decoded.count, chunk.byteLength)
            XCTAssertEqual(chunk.checksum, referenceCRC32(decoded))
            bytes.append(decoded)
            XCTAssertEqual(chunk.receivedBytes, startingBytes + bytes.count)
            index += 1
        }
        return (bytes, index - startingIndex)
    }

    private func referenceCRC32(_ bytes: Data) -> String {
        var crc: UInt32 = 0xffff_ffff
        for byte in bytes {
            crc ^= UInt32(byte)
            for _ in 0..<8 {
                crc = (crc >> 1) ^ (crc & 1 == 0 ? 0 : 0xedb8_8320)
            }
        }
        return String(format: "%08x", crc ^ 0xffff_ffff)
    }

    private func expectFailure(
        _ expected: MarkdownSourceUpload.TransferError,
        operation: () async throws -> Void
    ) async {
        do {
            try await operation()
            XCTFail("Expected source transfer failure: \(expected)")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceUpload.TransferError, expected)
        }
    }
}
