import Darwin
import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownBookImporterTests: XCTestCase {
    func testFolderImportPreservesNestedPathsSourceBytesAndEmptyChapters() async throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = "# Café 👩🏽‍💻\r\n\r\nExact e\u{301} and \u{FEFF} inside.\r\n"
        _ = try write("02-chapters/first.md", bytes: Data([0xEF, 0xBB, 0xBF]) + Data(source.utf8), in: root)
        _ = try write("01-empty.md", bytes: Data(), in: root)
        _ = try write("notes.txt", bytes: Data("not a chapter".utf8), in: root)
        _ = try write(".hidden.md", bytes: Data("not selected".utf8), in: root)

        let files = try await MarkdownBookImporter.load(from: [root])
        XCTAssertEqual(files.map(\.path), ["01-empty.md", "02-chapters/first.md"])
        XCTAssertEqual(files[0].source, "")
        XCTAssertEqual(Data(files[1].source.utf8), Data(source.utf8))
    }

    func testSelectionOrderDoesNotChangePublicationOrder() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let z = try write("z.md", bytes: Data("Z".utf8), in: root)
        let a = try write("a.md", bytes: Data("A".utf8), in: root)
        XCTAssertEqual(
            try MarkdownBookImporter.readSelection(from: [z, a]),
            try MarkdownBookImporter.readSelection(from: [a, z])
        )
    }

    func testDuplicateChapterNamesAcrossRootsFailInsteadOfDroppingSource() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let first = try write("first/chapter.md", bytes: Data("first source".utf8), in: root)
        let second = try write("second/chapter.md", bytes: Data("second source".utf8), in: root)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: [first, second])) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .duplicatePath("chapter.md"))
        }
    }

    func testNestedAndFlatPortableOutputNamesCannotCollide() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        _ = try write("part/chapter.md", bytes: Data("nested".utf8), in: root)
        _ = try write("part__chapter.md", bytes: Data("flat".utf8), in: root)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: [root])) {
            XCTAssertEqual(
                $0 as? MarkdownBookImporter.ImportError,
                .outputCollision("part/chapter.md", "part__chapter.md")
            )
        }
    }

    func testPortableOutputNamesAccountForExtensionsAndASCIICase() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let md = try write("one/chapter.md", bytes: Data(), in: root)
        let markdown = try write("two/chapter.markdown", bytes: Data(), in: root)
        let capitals = try write("three/Chapter.md", bytes: Data(), in: root)
        for pair in [[md, markdown], [md, capitals]] {
            XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: pair)) { error in
                guard let failure = error as? MarkdownBookImporter.ImportError else {
                    return XCTFail("Expected an import error, received \(error)")
                }
                guard case .outputCollision = failure else {
                    return XCTFail("Expected a portable publication collision, received \(error)")
                }
            }
        }
    }

    func testExactUnicodeRowAndSourceIdentitiesRemainDistinct() throws {
        let composed = BookSourceFile(path: "café.md", source: "é")
        let decomposed = BookSourceFile(path: "cafe\u{301}.md", source: "e\u{301}")
        XCTAssertNotEqual(composed.id, decomposed.id)
        XCTAssertNotEqual(composed, decomposed)
        XCTAssertEqual(Set([composed, decomposed]).count, 2)
        XCTAssertEqual(
            Data(try MarkdownBookImporter.canonicalPath(decomposed.path).utf8),
            Data(decomposed.path.utf8)
        )
        XCTAssertEqual(try MarkdownBookImporter.canonicalPath("a//./b/../c.md"), "a/c.md")
        XCTAssertEqual(try MarkdownBookImporter.canonicalPath("inside\u{FEFF}name.md"), "inside\u{FEFF}name.md")
    }

    func testPinnedRendererPathRewritesAreRefusedBeforeReading() {
        let rejected = [
            "", "/chapter.md", "../chapter.md", "chapter.md/", "chapter.MD",
            " chapter.md", "chapter.md ", "\\chapter.md", "a\\chapter.md",
            "a:chapter.md", "a\nchapter.md", "\u{FEFF}chapter.md", "chapter.md\u{FEFF}"
        ]
        for path in rejected {
            XCTAssertThrowsError(try MarkdownBookImporter.canonicalPath(path), path)
        }
    }

    func testAggregateBudgetIncludesPathsAndFilesBeforeDecoding() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let file = try write("a.md", bytes: Data([0xFF, 0xFF]), in: root)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(
            from: [file], limits: .init(chapters: 1, bytes: 5, entries: 10)
        )) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .tooLarge)
        }
        try Data("A".utf8).write(to: file)
        XCTAssertEqual(try MarkdownBookImporter.readSelection(
            from: [file], limits: .init(chapters: 1, bytes: 5, entries: 10)
        ).first?.source, "A")
        try Data().write(to: file)
        XCTAssertEqual(try MarkdownBookImporter.readSelection(
            from: [file], limits: .init(chapters: 1, bytes: 4, entries: 10)
        ).first?.source, "")
    }

    func testChapterAndTraversalLimitsApplyDuringDiscovery() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let a = try write("a.md", bytes: Data(), in: root)
        let b = try write("b.md", bytes: Data(), in: root)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(
            from: [a, b], limits: .init(chapters: 1, bytes: 100, entries: 10)
        )) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .tooManyFiles)
        }
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(
            from: [root], limits: .init(chapters: 1, bytes: 100, entries: 10)
        )) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .tooManyFiles)
        }
        _ = try write("ignored.txt", bytes: Data(), in: root)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(
            from: [root], limits: .init(chapters: 5, bytes: 100, entries: 2)
        )) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .tooManyEntries)
        }
    }

    func testInvalidUTF8AndEmptySelectionAreActionableFailures() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let file = try write("invalid.md", bytes: Data([0xF0, 0x9F]), in: root)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: [file])) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .invalidUTF8("invalid.md"))
        }
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: [])) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .noMarkdownFiles)
        }
        let text = try write("notes.txt", bytes: Data(), in: root)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: [text])) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .noMarkdownFiles)
        }
    }

    func testSymlinkFilesAndDirectoriesAreNotSilentlyImportedOrOmitted() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let target = try write("outside/chapter.md", bytes: Data("outside".utf8), in: root)
        let selected = root.appendingPathComponent("selected", isDirectory: true)
        try FileManager.default.createDirectory(at: selected, withIntermediateDirectories: true)
        let link = selected.appendingPathComponent("linked.md")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)
        for urls in [[link], [selected]] {
            XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: urls)) {
                XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .symbolicLink("linked.md"))
            }
        }
        let directoryLink = root.appendingPathComponent("directory-link")
        try FileManager.default.createSymbolicLink(at: directoryLink, withDestinationURL: selected)
        XCTAssertThrowsError(try MarkdownBookImporter.readSelection(from: [directoryLink])) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .symbolicLink("directory-link"))
        }
    }

    func testDescriptorTraversalCannotFollowReplacedDirectoryOrFinalSymlink() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let secret = try write("outside/secret.md", bytes: Data("outside source".utf8), in: root)
        let selected = root.appendingPathComponent("selected", isDirectory: true)
        try FileManager.default.createDirectory(at: selected, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(
            at: selected.appendingPathComponent("part"), withDestinationURL: secret.deletingLastPathComponent()
        )
        try FileManager.default.createSymbolicLink(
            at: selected.appendingPathComponent("secret.md"), withDestinationURL: secret
        )
        let descriptor = Darwin.open(selected.path, O_RDONLY | O_DIRECTORY)
        XCTAssertGreaterThanOrEqual(descriptor, 0)
        guard descriptor >= 0 else { return }
        defer { _ = Darwin.close(descriptor) }
        for components in [["part", "secret.md"], ["secret.md"], ["..", "outside", "secret.md"]] {
            XCTAssertThrowsError(try MarkdownBookImporter.openRelativeFile(
                components, in: descriptor, path: components.joined(separator: "/")
            ))
        }
    }

    func testSwappedFIFOIsOpenedWithoutWaitingAndRejectedBeforeReading() throws {
        let root = try makeDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let fifo = root.appendingPathComponent("queue.md")
        XCTAssertEqual(Darwin.mkfifo(fifo.path, mode_t(0o600)), 0)
        let descriptor = Darwin.open(root.path, O_RDONLY | O_DIRECTORY)
        XCTAssertGreaterThanOrEqual(descriptor, 0)
        guard descriptor >= 0 else { return }
        defer { _ = Darwin.close(descriptor) }
        let handle = try MarkdownBookImporter.openRelativeFile(["queue.md"], in: descriptor, path: "queue.md")
        defer { try? handle.close() }
        XCTAssertThrowsError(try MarkdownBookImporter.readRegularFile(handle, path: "queue.md", maximumBytes: 100)) {
            XCTAssertEqual($0 as? MarkdownBookImporter.ImportError, .notRegularFile("queue.md"))
        }
    }

    @MainActor
    func testFailedAndCancelledReplacementPreserveTheCompleteCollection() async throws {
        let files = [BookSourceFile(path: "original.md", source: "keep this")]
        let session = MarkdownBookImportSession { urls in
            if urls.first?.lastPathComponent == "failure" { throw MarkdownBookImporter.ImportError.noMarkdownFiles }
            if urls.first?.lastPathComponent == "cancel" { throw CancellationError() }
            return files
        }
        let owner = UUID()
        for name in ["success", "failure", "cancel"] {
            let request = session.beginSelection(owner: owner)
            let task = try XCTUnwrap(session.startImport(
                from: [URL(fileURLWithPath: "/\(name)")], request: request, currentOwner: { owner }
            ))
            await task.value
            XCTAssertEqual(session.files, files)
            XCTAssertFalse(session.isImporting)
            if name == "failure" { XCTAssertNotNil(session.errorMessage) }
            else { XCTAssertNil(session.errorMessage) }
        }
    }

    @MainActor
    func testNewestSelectionWinsEvenWhenCancelledOlderReadReturnsLate() async throws {
        let suspended = SuspendedBookImports()
        let session = MarkdownBookImportSession { try await suspended.read($0) }
        let owner = UUID()
        let firstURL = URL(fileURLWithPath: "/first")
        let secondURL = URL(fileURLWithPath: "/second")
        let first = session.beginSelection(owner: owner)
        let firstTask = try XCTUnwrap(session.startImport(from: [firstURL], request: first, currentOwner: { owner }))
        await suspended.waitUntilRequested(firstURL)
        let second = session.beginSelection(owner: owner)
        let secondTask = try XCTUnwrap(session.startImport(from: [secondURL], request: second, currentOwner: { owner }))
        await suspended.waitUntilRequested(secondURL)
        let newest = [BookSourceFile(path: "newest.md", source: "newest source")]
        await suspended.complete(secondURL, with: .success(newest))
        await secondTask.value
        await suspended.complete(firstURL, with: .failure(MarkdownBookImporter.ImportError.tooLarge))
        await firstTask.value
        XCTAssertEqual(session.files, newest)
        XCTAssertNil(session.errorMessage)
        XCTAssertFalse(session.isImporting)
    }

    @MainActor
    func testPickerRequestCannotStartTwoCompetingImports() async throws {
        let suspended = SuspendedBookImports()
        let session = MarkdownBookImportSession { try await suspended.read($0) }
        let owner = UUID()
        let url = URL(fileURLWithPath: "/one-pick")
        let request = session.beginSelection(owner: owner)
        let task = try XCTUnwrap(session.startImport(from: [url], request: request, currentOwner: { owner }))
        XCTAssertNil(session.startImport(from: [url], request: request, currentOwner: { owner }))
        await suspended.waitUntilRequested(url)
        let count = await suspended.requestCount
        XCTAssertEqual(count, 1)
        await suspended.complete(url, with: .success([BookSourceFile(path: "one.md", source: "one")]))
        await task.value
        XCTAssertEqual(session.files.map(\.path), ["one.md"])
    }

    @MainActor
    func testLiveDocumentOwnerChangeRejectsCompletionBeforeObserverDelivery() async throws {
        let suspended = SuspendedBookImports()
        let session = MarkdownBookImportSession { try await suspended.read($0) }
        var liveOwner = UUID()
        let url = URL(fileURLWithPath: "/old-document")
        let request = session.beginSelection(owner: liveOwner)
        let task = try XCTUnwrap(session.startImport(from: [url], request: request, currentOwner: { liveOwner }))
        await suspended.waitUntilRequested(url)
        liveOwner = UUID() // Deliberately do not call resetForDocument yet.
        await suspended.complete(url, with: .success([BookSourceFile(path: "old.md", source: "old")]))
        await task.value
        XCTAssertTrue(session.files.isEmpty)
        XCTAssertNil(session.errorMessage)
        XCTAssertFalse(session.isImporting)
    }

    @MainActor
    func testLeavingLabCancelsPublicationAndLateErrors() async throws {
        let suspended = SuspendedBookImports()
        let session = MarkdownBookImportSession { try await suspended.read($0) }
        let owner = UUID()
        let url = URL(fileURLWithPath: "/dismissed")
        let request = session.beginSelection(owner: owner)
        let task = try XCTUnwrap(session.startImport(from: [url], request: request, currentOwner: { owner }))
        await suspended.waitUntilRequested(url)
        session.cancelPending()
        await suspended.complete(url, with: .failure(MarkdownBookImporter.ImportError.tooLarge))
        await task.value
        XCTAssertTrue(session.files.isEmpty)
        XCTAssertFalse(session.isImporting)
        XCTAssertNil(session.errorMessage)
    }

    @MainActor
    func testPublicationSnapshotChecksLiveOwnerAndPendingSelection() async throws {
        let files = [BookSourceFile(path: "owned.md", source: "owned source")]
        let session = MarkdownBookImportSession { _ in files }
        let owner = UUID()
        let request = session.beginSelection(owner: owner)
        let task = try XCTUnwrap(session.startImport(
            from: [URL(fileURLWithPath: "/owned")], request: request, currentOwner: { owner }
        ))
        await task.value
        XCTAssertEqual(session.publicationFiles(owner: owner), files)
        _ = session.beginSelection(owner: owner)
        XCTAssertNil(session.publicationFiles(owner: owner), "A pending picker cannot export the previous choice")
        session.cancelPending()
        XCTAssertEqual(session.publicationFiles(owner: owner), files)
        XCTAssertNil(session.publicationFiles(owner: UUID()), "A new document cannot reuse the old collection")
        XCTAssertTrue(session.files.isEmpty)
    }

    @MainActor
    func testPickerReturnedAfterDocumentReplacementCannotStartReading() {
        let session = MarkdownBookImportSession { _ in
            XCTFail("A stale picker must never start filesystem work")
            return [BookSourceFile(path: "stale.md", source: "stale")]
        }
        let request = session.beginSelection(owner: UUID())
        XCTAssertNil(session.startImport(
            from: [URL(fileURLWithPath: "/stale")], request: request, currentOwner: { UUID() }
        ))
        XCTAssertFalse(session.isImporting)
        XCTAssertTrue(session.files.isEmpty)
    }

    private func makeDirectory() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("fmd-book-import-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func write(_ path: String, bytes: Data, in root: URL) throws -> URL {
        let file = root.appendingPathComponent(path)
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try bytes.write(to: file)
        return file
    }
}

private actor SuspendedBookImports {
    private var pending: [URL: [CheckedContinuation<[BookSourceFile], Error>]] = [:]
    private var observers: [URL: CheckedContinuation<Void, Never>] = [:]
    private(set) var requestCount = 0

    func read(_ urls: [URL]) async throws -> [BookSourceFile] {
        let url = urls[0]
        requestCount += 1
        return try await withCheckedThrowingContinuation { continuation in
            pending[url, default: []].append(continuation)
            observers.removeValue(forKey: url)?.resume()
        }
    }

    func waitUntilRequested(_ url: URL) async {
        if pending[url] != nil { return }
        await withCheckedContinuation { observers[url] = $0 }
    }

    func complete(_ url: URL, with result: Result<[BookSourceFile], Error>) {
        for continuation in pending.removeValue(forKey: url) ?? [] {
            continuation.resume(with: result)
        }
    }
}
