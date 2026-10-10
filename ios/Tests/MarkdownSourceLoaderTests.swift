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
        XCTAssertEqual(
            opened.suggestedTitle,
            url.deletingPathExtension().lastPathComponent
        )
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
    func testSessionSaveAdvancesBaselineForAutomaticAndManualSaves() async throws {
        let defaults = try makeDefaults()
        let original = "# Original\n"
        let updated = "# Updated\n\nSaved without leaving the editor.\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let session = MarkdownDocumentSession(initialSource: original, defaults: defaults)
        session.adopt(try await MarkdownSourceLoader.open(from: url))

        XCTAssertTrue(session.isDirty(source: updated))
        try await session.save(source: updated)

        XCTAssertFalse(session.isDirty(source: updated))
        XCTAssertEqual(try MarkdownSourceLoader.decode(Data(contentsOf: url)), updated)
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
        let reopenedResult = try await restored.openRecent(recent)
        let reopened = try XCTUnwrap(reopenedResult)
        XCTAssertEqual(reopened.source, "# Recent\n")
    }

    @MainActor
    func testSessionRestoresActiveFileWhenThereIsNoRecoveredDraft() async throws {
        let defaults = try makeDefaults()
        let original = "# Original\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let firstSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        firstSession.adopt(try await MarkdownSourceLoader.open(from: url))
        try Data("# Updated elsewhere\n".utf8).write(to: url, options: .atomic)

        let restoredSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: nil,
            recoveredDocumentIdentity: nil
        )

        guard case .fileVersion(let restored) = restoration else {
            return XCTFail("A launch without recovered edits should use the current file version")
        }
        XCTAssertEqual(restored.document.source, "# Updated elsewhere\n")
    }

    @MainActor
    func testSessionReconnectsRecoveredEditsWhenFileIsUnchanged() async throws {
        let defaults = try makeDefaults()
        let original = "# Original\n"
        let recovered = "# Original\n\nRecovered paragraph.\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let firstSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        firstSession.adopt(try await MarkdownSourceLoader.open(from: url))
        let documentIdentity = try XCTUnwrap(firstSession.currentDocumentIdentity)

        let restoredSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: recovered,
            recoveredDocumentIdentity: documentIdentity
        )
        guard case .recoveredEdits(let restored) = restoration else {
            return XCTFail("Recovered edits should reconnect to an unchanged file")
        }
        restoredSession.adoptRecoveredEdits(
            from: restored.document,
            documentIdentity: restored.documentIdentity,
            changedOnDisk: false
        )

        XCTAssertTrue(restoredSession.hasCurrentDocument)
        XCTAssertTrue(restoredSession.isDirty(source: recovered))
        XCTAssertNil(restoredSession.attention)
    }

    @MainActor
    func testSessionRefusesInPlaceSaveAfterTwoSidedRestorationConflict() async throws {
        let defaults = try makeDefaults()
        let original = "# Original\n"
        let external = "# External edit\n"
        let recovered = "# Recovered local edit\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let firstSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        firstSession.adopt(try await MarkdownSourceLoader.open(from: url))
        let documentIdentity = try XCTUnwrap(firstSession.currentDocumentIdentity)
        try Data(external.utf8).write(to: url, options: .atomic)

        let restoredSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: recovered,
            recoveredDocumentIdentity: documentIdentity
        )
        guard case .conflict(let restored) = restoration else {
            return XCTFail("Two changed versions must produce an explicit restoration conflict")
        }
        restoredSession.adoptRecoveredEdits(
            from: restored.document,
            documentIdentity: restored.documentIdentity,
            changedOnDisk: true
        )

        do {
            try await restoredSession.save(source: recovered)
            XCTFail("In-place Save must stay blocked while the restoration conflict is unresolved")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.DocumentError, .changedOnDisk)
        }
        XCTAssertEqual(try MarkdownSourceLoader.decode(Data(contentsOf: url)), external)
    }

    @MainActor
    func testBeginningUntitledClearsAutomaticDocumentRestoration() async throws {
        let defaults = try makeDefaults()
        let url = try temporarySourceURL(contents: Data("# Stored\n".utf8))
        let firstSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        firstSession.adopt(try await MarkdownSourceLoader.open(from: url))
        firstSession.beginUntitled(source: "# New\n")

        let restoredSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: "# New\n",
            recoveredDocumentIdentity: nil
        )
        XCTAssertEqual(restoration, .none)
    }

    @MainActor
    func testSessionDoesNotAttachDraftFromAnotherDocument() async throws {
        let defaults = try makeDefaults()
        let url = try temporarySourceURL(contents: Data("# Current file\n".utf8))
        let firstSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        firstSession.adopt(try await MarkdownSourceLoader.open(from: url))

        let restoredSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        let restoration = try await restoredSession.restoreActiveDocument(
            recoveredSource: "# Draft from a different file\n",
            recoveredDocumentIdentity: UUID()
        )

        guard case .unassociatedDraft = restoration else {
            return XCTFail("A draft with another identity must never attach to the active file")
        }
    }

    @MainActor
    func testNewerOpenWinsWhenReadsCompleteOutOfOrder() async throws {
        let first = try await testDocument("# First\n", name: "first.md")
        let second = try await testDocument("# Second\n", name: "second.md")
        let reads = SuspendedMarkdownIO()
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n",
            defaults: try makeDefaults(),
            openDocument: { try await reads.read($0) }
        )
        let firstRequest = session.beginDocumentRequest()
        let firstTask = Task { try await session.open(from: first.url, request: firstRequest) }
        await reads.waitUntilRequested(first.url)
        let secondRequest = session.beginDocumentRequest()
        let secondTask = Task { try await session.open(from: second.url, request: secondRequest) }
        await reads.waitUntilRequested(second.url)

        await reads.complete(second.url, with: .success(second))
        let secondResult = try await secondTask.value
        session.adopt(try XCTUnwrap(secondResult))
        await reads.complete(first.url, with: .success(first))

        let firstResult = try await firstTask.value
        XCTAssertNil(firstResult)
        XCTAssertEqual(session.currentDocument, second)
        XCTAssertEqual(session.recentDocuments.first?.displayName, "second.md")
    }

    @MainActor
    func testOpenReservedBeforeNewIsDiscardedEvenIfItsTaskStartsLate() async throws {
        let document = try await testDocument("# Old request\n")
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n",
            defaults: try makeDefaults(),
            openDocument: { _ in document }
        )
        let request = session.beginDocumentRequest()
        session.beginUntitled(source: "# New document\n")

        let result = try await session.open(from: document.url, request: request)

        XCTAssertNil(result)
        XCTAssertFalse(session.hasCurrentDocument)
        XCTAssertFalse(session.isDirty(source: "# New document\n"))
    }

    @MainActor
    func testFailedOlderOpenDoesNotReportAnErrorForTheNewRequest() async throws {
        let document = try await testDocument("# Old request\n")
        let reads = SuspendedMarkdownIO()
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n",
            defaults: try makeDefaults(),
            openDocument: { try await reads.read($0) }
        )
        let request = session.beginDocumentRequest()
        let task = Task { try await session.open(from: document.url, request: request) }
        await reads.waitUntilRequested(document.url)
        session.cancelPendingDocumentRequests()
        await reads.complete(document.url, with: .failure(MarkdownSourceLoader.DocumentError.coordinatedRead))

        let result = try await task.value
        XCTAssertNil(result)
        XCTAssertNil(session.attention)
    }

    @MainActor
    func testStartupRestorationCannotReplaceAnExplicitlyOpenedFile() async throws {
        let defaults = try makeDefaults()
        let previous = try await testDocument("# Previous file\n", name: "previous.md")
        let selected = try await testDocument("# Selected in Finder\n", name: "selected.md")
        let initialSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        initialSession.adopt(previous)
        let reads = SuspendedMarkdownIO()
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n",
            defaults: defaults,
            openDocument: { try await reads.read($0) }
        )
        let restorationTask = Task {
            try await session.restoreActiveDocument(
                recoveredSource: nil,
                recoveredDocumentIdentity: nil
            )
        }
        await reads.waitUntilRequested(previous.url)
        let request = session.beginDocumentRequest()
        let openTask = Task { try await session.open(from: selected.url, request: request) }
        await reads.waitUntilRequested(selected.url)
        await reads.complete(selected.url, with: .success(selected))
        let selectedResult = try await openTask.value
        session.adopt(try XCTUnwrap(selectedResult))
        await reads.complete(previous.url, with: .success(previous))

        let restoration = try await restorationTask.value
        XCTAssertEqual(restoration, .none)
        XCTAssertEqual(session.currentDocument, selected)
        XCTAssertNil(session.attention)
    }

    @MainActor
    func testEditingDuringStartupRestorationKeepsLiveBufferOnReadFailure() async throws {
        let defaults = try makeDefaults()
        let previous = try await testDocument("# Previous file\n")
        let initialSession = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        initialSession.adopt(previous)
        let reads = SuspendedMarkdownIO()
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n",
            defaults: defaults,
            openDocument: { try await reads.read($0) }
        )
        var currentSource = "# Untitled\n"
        let task = Task {
            try await session.restoreActiveDocument(
                recoveredSource: nil,
                recoveredDocumentIdentity: nil,
                isCurrentSource: { currentSource == "# Untitled\n" }
            )
        }
        await reads.waitUntilRequested(previous.url)
        currentSource = "# Typed while the document provider was busy\n"
        await reads.complete(previous.url, with: .failure(MarkdownSourceLoader.DocumentError.coordinatedRead))

        let result = try await task.value
        XCTAssertEqual(result, .none)
        XCTAssertFalse(session.hasCurrentDocument)
        XCTAssertNil(session.attention)
        XCTAssertTrue(session.isDirty(source: currentSource))
    }

    @MainActor
    func testSaveForPreviousDocumentCannotReattachItOrFinishNewDocumentsSave() async throws {
        let first = try await testDocument("# First\n", name: "first.md")
        let second = try await testDocument("# Second\n", name: "second.md")
        let writes = SuspendedMarkdownIO()
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n",
            defaults: try makeDefaults(),
            saveDocument: { _, document in try await writes.read(document.url) }
        )
        session.adopt(first)
        let firstSave = Task { try await session.save(source: "# First edited\n") }
        await writes.waitUntilRequested(first.url)
        session.adopt(second)
        let secondIdentity = session.currentDocumentIdentity
        let secondSave = Task { try await session.save(source: "# Second edited\n") }
        await writes.waitUntilRequested(second.url)

        await writes.complete(first.url, with: .success(first))
        try await firstSave.value
        XCTAssertEqual(session.currentDocument, second)
        XCTAssertEqual(session.currentDocumentIdentity, secondIdentity)
        XCTAssertTrue(session.isSaving, "The first save must not clear the second document's busy state")
        XCTAssertEqual(session.recentDocuments.first?.displayName, "second.md")

        await writes.complete(second.url, with: .success(second))
        try await secondSave.value
        XCTAssertFalse(session.isSaving)
        XCTAssertEqual(session.currentDocument, second)
    }

    @MainActor
    func testOldSaveFailureDoesNotMarkNewDocumentAsConflicted() async throws {
        let first = try await testDocument("# First\n", name: "first.md")
        let second = try await testDocument("# Second\n", name: "second.md")
        let writes = SuspendedMarkdownIO()
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n",
            defaults: try makeDefaults(),
            saveDocument: { _, document in try await writes.read(document.url) }
        )
        session.adopt(first)
        let task = Task { try await session.save(source: "# First edited\n") }
        await writes.waitUntilRequested(first.url)
        session.adopt(second)
        await writes.complete(first.url, with: .failure(MarkdownSourceLoader.DocumentError.changedOnDisk))
        do {
            try await task.value
            XCTFail("The requested save failure must still reach its caller")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.DocumentError, .changedOnDisk)
        }
        XCTAssertEqual(session.currentDocument, second)
        XCTAssertNil(session.attention)
        XCTAssertFalse(session.isSaving)
    }

    private func testDocument(_ source: String, name: String = "notes.md") async throws -> MarkdownSourceDocument {
        let url = try temporarySourceURL(contents: Data(source.utf8), name: name)
        return try await MarkdownSourceLoader.open(from: url)
    }

    private func makeDefaults() throws -> UserDefaults {
        try XCTUnwrap(UserDefaults(suiteName: "MarkdownRestorationTests.\(UUID().uuidString)"))
    }

    private func temporarySourceURL(contents: Data, name: String = "notes.md") throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("frankenmarkdown-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent(name, isDirectory: false)
        try contents.write(to: url, options: .atomic)
        return url
    }
}

/// Controlled suspension proves request ordering without timing sleeps or a
/// particular filesystem/provider speed. Production still uses coordinated I/O.
private actor SuspendedMarkdownIO {
    private var requests: [URL: CheckedContinuation<MarkdownSourceDocument, Error>] = [:]
    private var observers: [URL: CheckedContinuation<Void, Never>] = [:]

    func read(_ url: URL) async throws -> MarkdownSourceDocument {
        let key = url.standardizedFileURL.resolvingSymlinksInPath()
        return try await withCheckedThrowingContinuation { continuation in
            requests[key] = continuation
            observers.removeValue(forKey: key)?.resume()
        }
    }

    func waitUntilRequested(_ url: URL) async {
        let key = url.standardizedFileURL.resolvingSymlinksInPath()
        if requests[key] != nil { return }
        await withCheckedContinuation { continuation in
            observers[key] = continuation
        }
    }

    func complete(_ url: URL, with result: Result<MarkdownSourceDocument, Error>) {
        let key = url.standardizedFileURL.resolvingSymlinksInPath()
        requests.removeValue(forKey: key)?.resume(with: result)
    }
}
