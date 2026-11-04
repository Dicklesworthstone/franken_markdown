import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownExportWriterTests: XCTestCase {
    func testChecksumUsesStandardCRC32() {
        XCTAssertEqual(MarkdownExportWriter.checksum(Data("123456789".utf8)), "cbf43926")
        XCTAssertEqual(MarkdownExportWriter.checksum(Data()), "00000000")
    }

    func testExactBytesAcrossChunkBoundariesAndEmptyExports() async throws {
        for size in [0, 1, 65_535, 65_536, 65_537, 196_615] {
            let bytes = Data((0..<size).map { UInt8($0 % 251) })
            let sink = try MarkdownExportWriter(manifest: manifest(size), requestID: 7, title: "Document")
            for offset in stride(from: 0, to: size, by: 65_536) {
                let part = bytes.subdata(in: offset..<min(size, offset + 65_536))
                try await sink.append(chunk(part, index: offset / 65_536))
            }
            let artifact = try await sink.finish()
            XCTAssertEqual(artifact.byteLength, size)
            XCTAssertEqual(artifact.diagnosticCount, 2)
            XCTAssertEqual(try Data(contentsOf: artifact.fileURL), bytes)
            XCTAssertFalse(FileManager.default.fileExists(atPath:
                artifact.fileURL.deletingLastPathComponent().appendingPathComponent("transfer.partial").path))
            try FileManager.default.removeItem(at: artifact.fileURL.deletingLastPathComponent())
        }
    }

    func testUTF8ScalarsMayStraddleTransportChunksWithoutReencoding() async throws {
        let text = String(repeating: "x", count: 65_535) + "🧪 café 日本語\r\nEND"
        let bytes = Data(text.utf8)
        let sink = try MarkdownExportWriter(manifest: manifest(bytes.count), requestID: 7, title: "Unicode")
        try await sink.append(chunk(bytes.subdata(in: 0..<65_536), index: 0))
        try await sink.append(chunk(bytes.subdata(in: 65_536..<bytes.count), index: 1))
        let artifact = try await sink.finish()
        XCTAssertEqual(try String(contentsOf: artifact.fileURL, encoding: .utf8), text)
        try FileManager.default.removeItem(at: artifact.fileURL.deletingLastPathComponent())
    }

    func testInvalidManifestIsRefusedBeforeCreatingStorage() throws {
        for invalid in [
            manifest(-1), manifest(MarkdownExportWriter.maximumBytes + 1),
            manifest(4, requestID: 8), manifest(4, chunkSize: 1),
            manifest(4, extension: "../pdf"), manifest(4, mimeType: "application/zip"),
            manifest(4, extension: "../unknown", mimeType: ""),
            manifest(4, diagnosticCount: -1)
        ] {
            XCTAssertThrowsError(try MarkdownExportWriter(manifest: invalid, requestID: 7, title: "File")) {
                XCTAssertEqual($0 as? MarkdownExportWriter.TransferError, .invalidManifest)
            }
        }
    }

    func testCorruptOutOfOrderAndOversizedChunksDoNotChangeTheFile() async throws {
        let bytes = Data("payload".utf8)
        let valid = chunk(bytes, index: 0)
        let sink = try MarkdownExportWriter(manifest: manifest(bytes.count), requestID: 7, title: "Checked")
        let invalid = [
            chunk(bytes, index: 1), chunk(bytes, index: -1), chunk(bytes, index: 0, requestID: 8),
            MarkdownExportChunk(requestID: 7, index: 0, base64: "!!!!!!!!!!!!", byteLength: bytes.count, checksum: valid.checksum),
            MarkdownExportChunk(requestID: 7, index: 0, base64: valid.base64, byteLength: bytes.count, checksum: "00000000"),
            MarkdownExportChunk(requestID: 7, index: 0, base64: valid.base64, byteLength: bytes.count + 1, checksum: valid.checksum),
            chunk(Data(repeating: 97, count: 65_537), index: 0)
        ]
        for part in invalid {
            await expectFailure(.invalidChunk) { try await sink.append(part) }
        }
        try await sink.append(valid)
        await expectFailure(.invalidChunk) { try await sink.append(valid) }
        let artifact = try await sink.finish()
        XCTAssertEqual(try Data(contentsOf: artifact.fileURL), bytes)
        await expectFailure(.closed) { try await sink.append(valid) }
        await expectFailure(.closed) { _ = try await sink.finish() }
        try FileManager.default.removeItem(at: artifact.fileURL.deletingLastPathComponent())
    }

    func testIncompleteTransferCannotPublishAndAbortRemovesPartialStorage() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        let sink = try MarkdownExportWriter(
            manifest: manifest(100_000), requestID: 7, title: "Incomplete", temporaryDirectory: directory
        )
        try await sink.append(chunk(Data(repeating: 97, count: 65_536), index: 0))
        await expectFailure(.incomplete) { _ = try await sink.finish() }
        await sink.abort()
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: directory.path), [])
        await expectFailure(.closed) { _ = try await sink.finish() }
        try FileManager.default.removeItem(at: directory)
    }

    func testEachExportHasAnIndependentSafeFilename() async throws {
        let title = "../../" + String(repeating: "🧪", count: 200)
        let first = try MarkdownExportWriter(manifest: manifest(0), requestID: 7, title: title)
        let second = try MarkdownExportWriter(manifest: manifest(0), requestID: 7, title: title)
        let a = try await first.finish()
        let b = try await second.finish()
        XCTAssertNotEqual(a.fileURL, b.fileURL)
        XCTAssertLessThan(a.fileURL.lastPathComponent.utf8.count, 150)
        XCTAssertEqual(a.fileURL.pathExtension, "pdf")
        XCTAssertTrue(a.fileURL.deletingLastPathComponent().lastPathComponent.hasPrefix("FrankenMarkdown-Export-"))
        try FileManager.default.removeItem(at: a.fileURL.deletingLastPathComponent())
        try FileManager.default.removeItem(at: b.fileURL.deletingLastPathComponent())
    }

    private func expectFailure(
        _ expected: MarkdownExportWriter.TransferError,
        operation: () async throws -> Void
    ) async {
        do {
            try await operation()
            XCTFail("Expected transfer failure: \(expected)")
        } catch { XCTAssertEqual(error as? MarkdownExportWriter.TransferError, expected) }
    }

    private func manifest(
        _ count: Int, requestID: Int = 7, chunkSize: Int = 65_536,
        extension ext: String = "pdf", mimeType: String = "application/pdf", diagnosticCount: Int = 2
    ) -> MarkdownExportManifest {
        MarkdownExportManifest(requestID: requestID, byteLength: count, extension: ext,
            mimeType: mimeType, diagnosticCount: diagnosticCount, chunkSize: chunkSize)
    }

    private func chunk(_ bytes: Data, index: Int, requestID: Int = 7) -> MarkdownExportChunk {
        MarkdownExportChunk(requestID: requestID, index: index, base64: bytes.base64EncodedString(),
            byteLength: bytes.count, checksum: MarkdownExportWriter.checksum(bytes))
    }
}
