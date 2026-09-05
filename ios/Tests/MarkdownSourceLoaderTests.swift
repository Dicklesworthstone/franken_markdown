import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownSourceLoaderTests: XCTestCase {
    func testDecodeAcceptsUTF8Markdown() throws {
        let markdown = "# Café\n\nA real document."

        XCTAssertEqual(
            try MarkdownSourceLoader.decode(Data(markdown.utf8)),
            markdown
        )
    }

    func testDecodeAndEncodePreserveExplicitUTF8ByteOrderMark() throws {
        let markdown = "# Café\n\nA real document."
        let encoded = try MarkdownSourceLoader.encode(markdown, includingByteOrderMark: true)

        XCTAssertTrue(encoded.starts(with: Data([0xEF, 0xBB, 0xBF])))
        XCTAssertEqual(try MarkdownSourceLoader.decode(encoded), markdown)
    }

    func testDecodeRejectsInputPastTheBound() {
        XCTAssertThrowsError(
            try MarkdownSourceLoader.decode(Data(repeating: 0x61, count: 5), maximumBytes: 4)
        ) { error in
            XCTAssertEqual(
                error as? MarkdownSourceLoader.ImportError,
                .tooLarge(maximumBytes: 4)
            )
        }
    }

    func testDecodeRejectsInvalidUTF8() {
        XCTAssertThrowsError(try MarkdownSourceLoader.decode(Data([0xC3, 0x28]))) { error in
            XCTAssertEqual(error as? MarkdownSourceLoader.ImportError, .notUTF8)
        }
    }

    func testOpenAndSaveRoundTripPreservesIdentityAndByteOrderMark() async throws {
        let original = "# Original\n\nStored in Files."
        let updated = "# Updated\n\nStill stored in Files."
        let url = try temporarySourceURL(
            contents: Data([0xEF, 0xBB, 0xBF]) + Data(original.utf8)
        )

        let opened = try await MarkdownSourceLoader.open(from: url)
        let saved = try await MarkdownSourceLoader.save(updated, replacing: opened)

        XCTAssertEqual(opened.url, url)
        XCTAssertEqual(opened.source, original)
        XCTAssertEqual(opened.suggestedTitle, "notes")
        XCTAssertEqual(saved.url, url)
        XCTAssertEqual(saved.source, updated)
        XCTAssertTrue(saved.diskData.starts(with: Data([0xEF, 0xBB, 0xBF])))
        XCTAssertEqual(try MarkdownSourceLoader.decode(Data(contentsOf: url)), updated)
    }

    func testSaveRefusesToOverwriteExternalChange() async throws {
        let original = "# Original\n"
        let external = "# External edit\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let opened = try await MarkdownSourceLoader.open(from: url)
        try Data(external.utf8).write(to: url, options: .atomic)

        do {
            _ = try await MarkdownSourceLoader.save("# Local edit\n", replacing: opened)
            XCTFail("Save must not overwrite a file whose bytes changed after opening")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.DocumentError, .changedOnDisk)
        }
        XCTAssertEqual(try MarkdownSourceLoader.decode(Data(contentsOf: url)), external)
    }

    @MainActor
    func testSessionKeepsConflictVisibleUntilReopen() async throws {
        let defaults = try XCTUnwrap(
            UserDefaults(suiteName: "MarkdownSourceConflictTests.\(UUID().uuidString)")
        )
        let source = "# Original\n"
        let url = try temporarySourceURL(contents: Data(source.utf8))
        let session = MarkdownDocumentSession(initialSource: source, defaults: defaults)
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        try Data("# External edit\n".utf8).write(to: url, options: .atomic)

        do {
            try await session.save(source: "# Local edit\n")
            XCTFail("Session save must surface the external-file conflict")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.DocumentError, .changedOnDisk)
        }
        XCTAssertEqual(session.attention, .changedOnDisk)

        session.adopt(try await MarkdownSourceLoader.open(from: url))
        XCTAssertNil(session.attention)
    }

    @MainActor
    func testSessionTracksDirtyStateAndPersistsBoundedRecents() async throws {
        let suiteName = "MarkdownSourceLoaderTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        let initial = "# Untitled\n"
        let session = MarkdownDocumentSession(initialSource: initial, defaults: defaults)

        XCTAssertFalse(session.isDirty(source: initial))
        XCTAssertTrue(session.isDirty(source: initial + "More\n"))

        var newestName = ""
        for index in 0..<(MarkdownDocumentSession.maximumRecentDocuments + 2) {
            let name = "recent-\(index).md"
            newestName = name
            let url = try temporarySourceURL(contents: Data("# Recent\n".utf8), name: name)
            session.adopt(try await MarkdownSourceLoader.open(from: url))
        }

        XCTAssertEqual(session.recentDocuments.count, MarkdownDocumentSession.maximumRecentDocuments)
        XCTAssertEqual(session.recentDocuments.first?.displayName, newestName)
        let restored = MarkdownDocumentSession(initialSource: initial, defaults: defaults)
        XCTAssertEqual(restored.recentDocuments, session.recentDocuments)
        let recent = try XCTUnwrap(restored.recentDocuments.first)
        XCTAssertEqual(try await restored.openRecent(recent).source, "# Recent\n")
    }

    private func temporarySourceURL(contents: Data, name: String = "notes.md") throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("frankenmarkdown-tests-\(UUID().uuidString)-\(name)")
        try contents.write(to: url, options: .atomic)
        return url
    }
}
