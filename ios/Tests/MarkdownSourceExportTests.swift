import CryptoKit
import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownSourceExportTests: XCTestCase {
    func testUTF8AndBOMArePreservedAcrossChunkBoundaries() throws {
        let source = String(repeating: "a", count: MarkdownSourceLoader.ioChunkBytes - 1)
            + "👩🏽‍💻\r\n漢字\u{FEFF}\0e\u{301}\n"
        for hasBOM in [false, true] {
            var delivered = Data()
            var largest = 0
            let result = try MarkdownSourceExport.writeSource(source, includingByteOrderMark: hasBOM) {
                largest = max(largest, $0.count)
                delivered.append($0)
            }
            let expected = (hasBOM ? Data([0xEF, 0xBB, 0xBF]) : Data()) + Data(source.utf8)
            XCTAssertEqual(delivered, expected)
            XCTAssertEqual(result.byteCount, expected.count)
            XCTAssertEqual(result.sourceDigest, Data(SHA256.hash(data: Data(source.utf8))))
            XCTAssertLessThanOrEqual(largest, MarkdownSourceLoader.ioChunkBytes)
        }
    }

    func testLargeSourceIsDeliveredInBoundedWrites() throws {
        let source = String(repeating: "line👩🏽‍💻\r\n", count: 500_000)
        var bytes = 0
        var largest = 0
        var writes = 0
        let result = try MarkdownSourceExport.writeSource(source, includingByteOrderMark: false) {
            bytes += $0.count
            largest = max(largest, $0.count)
            writes += 1
        }
        XCTAssertGreaterThan(source.utf8.count, 8 * 1_024 * 1_024)
        XCTAssertEqual(bytes, source.utf8.count)
        XCTAssertEqual(result.byteCount, bytes)
        XCTAssertEqual(result.sourceDigest, Data(SHA256.hash(data: Data(source.utf8))))
        XCTAssertLessThanOrEqual(largest, 64 * 1_024)
        XCTAssertGreaterThan(writes, 100)
    }

    func testSizeLimitIncludesBOMAndRejectsBeforeWriting() throws {
        var writes = 0
        XCTAssertThrowsError(try MarkdownSourceExport.writeSource(
            "🧪", includingByteOrderMark: true, maximumBytes: 6
        ) { _ in writes += 1 }) { error in
            XCTAssertEqual(error as? MarkdownSourceLoader.ImportError, .tooLarge(maximumBytes: 6))
        }
        XCTAssertEqual(writes, 0)
        let result = try MarkdownSourceExport.writeSource(
            "🧪", includingByteOrderMark: true, maximumBytes: 7
        ) { _ in writes += 1 }
        XCTAssertEqual(result.byteCount, 7)
        XCTAssertEqual(writes, 2)
    }

    func testWriteFailureStopsBeforeTheRemainingSource() {
        enum Failure: Error { case diskFull }
        var writes = 0
        let source = String(repeating: "a", count: 4 * MarkdownSourceLoader.ioChunkBytes)
        XCTAssertThrowsError(try MarkdownSourceExport.writeSource(source, includingByteOrderMark: false) { _ in
            writes += 1
            if writes == 2 { throw Failure.diskFull }
        }) { error in
            guard case Failure.diskFull = error else {
                return XCTFail("Expected the original sink error, got \(error)")
            }
        }
        XCTAssertEqual(writes, 2)
    }

    func testEmptyAndBOMOnlySource() throws {
        for hasBOM in [false, true] {
            var bytes = Data()
            let result = try MarkdownSourceExport.writeSource("", includingByteOrderMark: hasBOM) {
                bytes.append($0)
            }
            XCTAssertEqual(bytes, hasBOM ? Data([0xEF, 0xBB, 0xBF]) : Data())
            XCTAssertEqual(result.byteCount, bytes.count)
            XCTAssertEqual(result.sourceDigest, Data(SHA256.hash(data: Data())))
        }
    }

    func testPreparedFileRoundTripsAndOnlyItsOwnDirectoryIsReleased() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let sentinel = root.appendingPathComponent("unrelated.md")
        try Data("keep".utf8).write(to: sentinel)
        let source = "# Café\r\n\nText\u{FEFF} and 🧪."
        var first: MarkdownSourceExport? = try MarkdownSourceExport.prepareSynchronously(
            source: source, includingByteOrderMark: true, temporaryDirectory: root
        )
        let firstURL = try XCTUnwrap(first).fileURL
        let bytes = try Data(contentsOf: firstURL)
        XCTAssertEqual(bytes, Data([0xEF, 0xBB, 0xBF]) + Data(source.utf8))
        XCTAssertEqual(try XCTUnwrap(first).byteCount, bytes.count)
        XCTAssertEqual(try MarkdownSourceLoader.decode(bytes).utf8.map { $0 }, Array(source.utf8))
        let second = try MarkdownSourceExport.prepareSynchronously(
            source: "second", includingByteOrderMark: false, temporaryDirectory: root
        )
        first = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: firstURL.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: second.fileURL.path))
        XCTAssertEqual(try Data(contentsOf: sentinel), Data("keep".utf8))
        withExtendedLifetime(second) {}
    }

    func testLiteralLeadingByteOrderMarkSurvivesReopening() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = "\u{FEFF}A literal source character\n"
        let snapshot = try MarkdownSourceExport.prepareSynchronously(
            source: source, includingByteOrderMark: false, temporaryDirectory: root
        )
        let bytes = try Data(contentsOf: snapshot.fileURL)
        XCTAssertTrue(bytes.starts(with: Data([0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF])))
        XCTAssertEqual(Data(try MarkdownSourceLoader.decode(bytes).utf8), Data(source.utf8))
        XCTAssertEqual(snapshot.sourceDigest, Data(SHA256.hash(data: Data(source.utf8))))
    }

    func testCancelledPreparationCreatesNoTemporaryFile() async throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let (stream, continuation) = AsyncStream<Void>.makeStream()
        let worker = Task.detached {
            for await _ in stream {}
            return try await MarkdownSourceExport.prepare(
                source: "cancelled", includingByteOrderMark: false, temporaryDirectory: root
            )
        }
        worker.cancel()
        continuation.finish()
        do {
            _ = try await worker.value
            XCTFail("Cancelled preparation must fail")
        } catch is CancellationError {
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
        }
    }

    private func temporaryDirectory() throws -> URL {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MarkdownSourceExportTests-\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        return directory
    }
}
