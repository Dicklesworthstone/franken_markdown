import CryptoKit
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

    func testStreamingReadPreservesEveryUTF8AndBOMBoundaryAcrossShortReads() throws {
        let source = "# Café 👩🏽‍💻\r\n漢字 e\u{301}\nValid \u{FFFD} scalar.\n\0Tail"
        for hasBOM in [false, true] {
            let bytes = (hasBOM ? Data([0xEF, 0xBB, 0xBF]) : Data()) + Data(source.utf8)
            for width in 1...17 {
                let loaded = try readInChunks(bytes, width: width)
                XCTAssertEqual(Data(loaded.source.utf8), Data(source.utf8), "width \(width), BOM \(hasBOM)")
                XCTAssertEqual(loaded.diskFingerprint.byteCount, bytes.count)
                XCTAssertEqual(loaded.diskFingerprint.digest, Data(SHA256.hash(data: bytes)))
                XCTAssertEqual(loaded.diskFingerprint.hasByteOrderMark, hasBOM)
                XCTAssertEqual(loaded.sourceDigest, Data(SHA256.hash(data: Data(source.utf8))))
            }
        }
    }

    func testStreamingReadPreservesEmptyAndBOMOnlyFiles() throws {
        for bytes in [Data(), Data([0xEF, 0xBB, 0xBF])] {
            let loaded = try readInChunks(bytes, width: 1)
            XCTAssertEqual(loaded.source, "")
            XCTAssertEqual(loaded.diskFingerprint.byteCount, bytes.count)
            XCTAssertEqual(loaded.diskFingerprint.hasByteOrderMark, !bytes.isEmpty)
            XCTAssertEqual(loaded.sourceDigest, Data(SHA256.hash(data: Data())))
        }
    }

    func testDecodeRemovesOnlyTheOriginalBOMAndPreservesLeadingSourceBOM() throws {
        let source = "\u{FEFF}Second mark is content.\nInterior \u{FEFF} mark."
        let bytes = Data([0xEF, 0xBB, 0xBF]) + Data(source.utf8)
        let decoded = try MarkdownSourceLoader.decode(bytes)
        XCTAssertEqual(Data(decoded.utf8), Data(source.utf8))
        for width in 1...9 {
            let loaded = try readInChunks(bytes, width: width)
            XCTAssertEqual(Data(loaded.source.utf8), Data(source.utf8))
            XCTAssertEqual(loaded.sourceDigest, Data(SHA256.hash(data: Data(source.utf8))))
        }
    }

    func testRawStreamingReadPreservesLeadingMarksAcrossEveryShortRead() throws {
        for source in ["\u{FEFF}", "\u{FEFF}\u{FEFF}# Literal marks\r\nCafé 👩🏽‍💻\nInterior \u{FEFF}"] {
            let bytes = Data(source.utf8)
            for width in 1...17 {
                let loaded = try readInChunks(bytes, width: width, stripInitialByteOrderMark: false)
                XCTAssertTrue(loaded.source.utf8.elementsEqual(source.utf8), "width \(width)")
                XCTAssertEqual(loaded.diskFingerprint.byteCount, bytes.count)
                XCTAssertEqual(loaded.diskFingerprint.digest, Data(SHA256.hash(data: bytes)))
                XCTAssertEqual(loaded.sourceDigest, loaded.diskFingerprint.digest)
            }
        }
        for incomplete in [Data([0xEF]), Data([0xEF, 0xBB])] {
            XCTAssertThrowsError(try readInChunks(incomplete, width: 1, stripInitialByteOrderMark: false))
        }
    }

    func testStreamingReadPreservesInteriorBOMAtAndAcrossIOChunkBoundaries() throws {
        let chunkBytes = MarkdownSourceLoader.ioChunkBytes
        for hasBOM in [false, true] {
            for offset in (chunkBytes - 2)...(chunkBytes + 1) {
                let prefixCount = offset - (hasBOM ? 3 : 0)
                let source = String(repeating: "a", count: prefixCount)
                    + "\u{FEFF}Middle\u{FEFF}Tail"
                let bytes = (hasBOM ? Data([0xEF, 0xBB, 0xBF]) : Data()) + Data(source.utf8)
                let loaded = try readInChunks(bytes, width: chunkBytes)
                XCTAssertEqual(Data(loaded.source.utf8), Data(source.utf8), "offset \(offset), BOM \(hasBOM)")
                XCTAssertEqual(loaded.sourceDigest, Data(SHA256.hash(data: Data(source.utf8))))
            }
        }
    }

    func testStreamingReadRejectsMalformedOrIncompleteScalarsAtEveryBoundary() {
        let invalid: [[UInt8]] = [
            [0x80], [0xC0, 0xAF], [0xC3, 0x28], [0xC3], [0xEF, 0xBB],
            [0xE0, 0x80, 0x80], [0xED, 0xA0, 0x80], [0xF0, 0x9F, 0x98],
            [0xF4, 0x90, 0x80, 0x80], [0xF5, 0x80, 0x80, 0x80]
        ]
        for bytes in invalid {
            for width in 1...4 {
                XCTAssertThrowsError(try readInChunks(Data(bytes), width: width)) { error in
                    XCTAssertEqual(error as? MarkdownSourceLoader.ImportError, .notUTF8)
                }
            }
        }
    }

    func testStreamingReadChecksActualLengthAndProbesPastExactLimit() throws {
        let bytes = Data("abcdefgh".utf8)
        XCTAssertEqual(try readInChunks(bytes, width: 3, maximumBytes: 8).source, "abcdefgh")
        XCTAssertThrowsError(try readInChunks(bytes, width: 3, maximumBytes: 7)) { error in
            XCTAssertEqual(error as? MarkdownSourceLoader.ImportError, .tooLarge(maximumBytes: 7))
        }
        let bomOnly = Data([0xEF, 0xBB, 0xBF])
        XCTAssertThrowsError(try readInChunks(bomOnly, width: 1, maximumBytes: 2)) { error in
            XCTAssertEqual(error as? MarkdownSourceLoader.ImportError, .tooLarge(maximumBytes: 2))
        }
    }

    func testMultiMegabyteStreamingReadBoundsEachReadAndRetainsTail() throws {
        let source = String(repeating: "A café 漢字 👩🏽‍💻 paragraph.\r\n", count: 100_000) + "FINAL-TAIL"
        let bytes = Data(source.utf8)
        var offset = 0
        var largestRequest = 0
        var reads = 0
        let loaded = try MarkdownSourceLoader.readSource(capacityHint: bytes.count) { count in
            largestRequest = max(largestRequest, count)
            reads += 1
            let end = min(offset + count, bytes.count)
            defer { offset = end }
            return bytes.subdata(in: offset..<end)
        }
        XCTAssertEqual(Data(loaded.source.utf8), bytes)
        XCTAssertEqual(loaded.diskFingerprint.digest, Data(SHA256.hash(data: bytes)))
        XCTAssertEqual(loaded.sourceDigest, try MarkdownSourceLoader.sourceDigest(source))
        XCTAssertLessThanOrEqual(largestRequest, MarkdownSourceLoader.ioChunkBytes)
        XCTAssertGreaterThan(reads, 2)
    }

    func testStreamingReadPropagatesProviderFailureWithoutReturningAPartialDocument() {
        var reads = 0
        XCTAssertThrowsError(try MarkdownSourceLoader.readSource { _ in
            reads += 1
            if reads == 1 { return Data("partial".utf8) }
            throw CocoaError(.fileReadUnknown)
        }) { error in
            XCTAssertEqual((error as? CocoaError)?.code, .fileReadUnknown)
        }
    }

    func testStreamingSaveWritesCompleteLargeSourceAndRejectsSameSizeExternalTailEdit() async throws {
        let source = String(repeating: "source café 漢字 🪄\n", count: 60_000) + "ORIGINAL"
        let updated = String(repeating: "updated café 漢字 🪄\n", count: 65_000) + "TAIL"
        let url = try temporarySourceURL(contents: try MarkdownSourceLoader.encode(source, includingByteOrderMark: true))
        let opened = try await MarkdownSourceLoader.open(from: url)
        let saved = try await MarkdownSourceLoader.save(updated, replacing: opened)
        let expectedBytes = try MarkdownSourceLoader.encode(updated, includingByteOrderMark: true)
        XCTAssertEqual(try Data(contentsOf: url), expectedBytes)
        XCTAssertEqual(saved.diskFingerprint.byteCount, expectedBytes.count)
        XCTAssertEqual(saved.diskFingerprint.digest, Data(SHA256.hash(data: expectedBytes)))
        XCTAssertEqual(saved.sourceDigest, Data(SHA256.hash(data: Data(updated.utf8))))

        var external = expectedBytes
        external[external.count - 1] ^= 1
        try external.write(to: url, options: .atomic)
        do {
            _ = try await MarkdownSourceLoader.save("local edit", replacing: saved)
            XCTFail("A same-size edit at the end of a large file must remain a conflict")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.DocumentError, .changedOnDisk)
        }
        XCTAssertEqual(try Data(contentsOf: url), external)
    }

    func testStreamingReadStopsAtTheNextBoundaryAfterTaskCancellation() async {
        let worker = Task.detached { () -> Bool in
            var reads = 0
            do {
                _ = try MarkdownSourceLoader.readSource { _ in
                    reads += 1
                    withUnsafeCurrentTask { $0?.cancel() }
                    return Data("partial".utf8)
                }
                return false
            } catch is CancellationError {
                return reads == 1
            } catch {
                return false
            }
        }
        let stoppedAfterOneRead = await worker.value
        XCTAssertTrue(stoppedAfterOneRead)
    }

    func testAlreadyCancelledOpenAndSaveDoNotAccessOrReplaceFiles() async throws {
        let source = "Original source."
        let url = try temporarySourceURL(contents: Data(source.utf8))
        let document = try await MarkdownSourceLoader.open(from: url)
        let nonexistent = url.appendingPathExtension("does-not-exist")
        let openWorker = Task.detached {
            withUnsafeCurrentTask { $0?.cancel() }
            return try await MarkdownSourceLoader.open(from: nonexistent)
        }
        do {
            _ = try await openWorker.value
            XCTFail("A cancelled open must not continue reading")
        } catch {
            XCTAssertTrue(error is CancellationError, "Cancellation must precede filesystem access: \(error)")
        }
        let saveWorker = Task.detached {
            withUnsafeCurrentTask { $0?.cancel() }
            return try await MarkdownSourceLoader.save("Unexpected replacement", replacing: document)
        }
        do {
            _ = try await saveWorker.value
            XCTFail("A cancelled save must not replace the file")
        } catch {
            XCTAssertTrue(error is CancellationError)
        }
        XCTAssertEqual(try Data(contentsOf: url), Data(source.utf8))
    }

    private func readInChunks(
        _ bytes: Data,
        width: Int,
        maximumBytes: Int = MarkdownSourceLoader.maximumBytes,
        stripInitialByteOrderMark: Bool = true
    ) throws -> MarkdownSourceLoader.LoadedSource {
        var offset = 0
        return try MarkdownSourceLoader.readSource(
            maximumBytes: maximumBytes,
            stripInitialByteOrderMark: stripInitialByteOrderMark
        ) { count in
            let end = min(bytes.count, offset + min(width, count))
            defer { offset = end }
            return bytes.subdata(in: offset..<end)
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
        let resolved = try MarkdownSourceLoader.resolveBookmark(saved.bookmarkData)
        let reopened = try await MarkdownSourceLoader.open(from: resolved)
        XCTAssertEqual(Data(reopened.source.utf8), Data(updated.utf8))
        XCTAssertTrue(saved.diskFingerprint.hasByteOrderMark)
        XCTAssertTrue(try Data(contentsOf: url).starts(with: Data([0xEF, 0xBB, 0xBF])))
        XCTAssertEqual(try MarkdownSourceLoader.decode(Data(contentsOf: url)), updated)
    }

    func testSavePreservesNewLiteralLeadingMarkWhenOriginalHadNoBOM() async throws {
        let url = try temporarySourceURL(contents: Data("# Original\n".utf8))
        let opened = try await MarkdownSourceLoader.open(from: url)
        XCTAssertFalse(opened.diskFingerprint.hasByteOrderMark)
        let source = "\u{FEFF}# Literal leading mark\r\nCafé 👩🏽‍💻\n"

        let saved = try await MarkdownSourceLoader.save(source, replacing: opened)

        XCTAssertTrue(saved.diskFingerprint.hasByteOrderMark)
        XCTAssertEqual(try Data(contentsOf: url), Data([0xEF, 0xBB, 0xBF]) + Data(source.utf8))
        XCTAssertEqual(saved.sourceDigest, Data(SHA256.hash(data: Data(source.utf8))))
        let reopened = try await MarkdownSourceLoader.open(from: url)
        XCTAssertTrue(reopened.source.utf8.elementsEqual(source.utf8))
        XCTAssertEqual(reopened.diskFingerprint, saved.diskFingerprint)

        // The new encoding marker is part of the saved file baseline, so a
        // subsequent edit can use the returned document without a false conflict.
        let nextSource = "# Without the literal mark\r\n"
        let next = try await MarkdownSourceLoader.save(nextSource, replacing: saved)
        XCTAssertTrue(next.diskFingerprint.hasByteOrderMark)
        XCTAssertEqual(try Data(contentsOf: url), Data([0xEF, 0xBB, 0xBF]) + Data(nextSource.utf8))
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
    func testDirtyStateAndSavePreserveCanonicallyEquivalentByteEdits() async throws {
        let original = "# \u{00E9}e\u{0301}\n"
        let edited = "# e\u{0301}\u{00E9}\n"
        XCTAssertEqual(original, edited, "The fixture must be canonically equivalent")
        XCTAssertEqual(original.utf8.count, edited.utf8.count)
        XCTAssertFalse(original.utf8.elementsEqual(edited.utf8))
        let session = MarkdownDocumentSession(initialSource: original, defaults: try makeDefaults())
        XCTAssertTrue(session.isDirty(source: edited), "Untitled drafts also preserve exact bytes")
        let url = try temporarySourceURL(contents: Data(original.utf8))
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        XCTAssertFalse(session.isDirty(source: original))
        XCTAssertTrue(session.isDirty(source: edited))

        try await session.save(source: edited)

        XCTAssertEqual(try Data(contentsOf: url), Data(edited.utf8))
        XCTAssertFalse(session.isDirty(source: edited))
        XCTAssertTrue(session.isDirty(source: original))
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

    @MainActor
    func testSlowSaveCoalescesLatestSourceAndUsesCommittedFingerprint() async throws {
        let original = "# Original\r\nCafé\r\n"
        let firstSource = "\u{FEFF}# First edit\r\n"
        let latestSource = "\u{FEFF}# Latest 👩🏽‍💻\r\ne\u{301}\r\n"
        let bom = Data([0xEF, 0xBB, 0xBF])
        let url = try temporarySourceURL(contents: bom + Data(original.utf8))
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: original,
            defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        let identity = session.currentDocumentIdentity
        let first = await MarkdownSaveCall.start(session, source: firstSource)
        _ = await writes.waitForRequest(0)
        // An explicit Save can queue behind slow autosave I/O, then a later
        // debounced edit supersedes that pending snapshot with the latest text.
        let supersededManual = await MarkdownSaveCall.start(session, source: "# Intermediate manual save\n")
        let latest = await MarkdownSaveCall.start(session, source: latestSource)
        guard await assertCancelled(supersededManual), assertPending(latest) else { return }
        let countBeforeCompletion = await writes.requestCount
        XCTAssertEqual(countBeforeCompletion, 1, "No second physical write may overlap this document's first save")

        await writes.release(0)
        let secondWrite = await writes.waitForRequest(1)
        try await first.value()
        XCTAssertTrue(session.isSaving, "The first completion must not clear the queued save's busy state")
        XCTAssertEqual(Data(secondWrite.source.utf8), Data(latestSource.utf8))
        XCTAssertEqual(Data(secondWrite.baseline.source.utf8), Data(firstSource.utf8))
        XCTAssertEqual(secondWrite.baseline.diskFingerprint.digest, Data(SHA256.hash(data: bom + Data(firstSource.utf8))))
        XCTAssertTrue(secondWrite.baseline.diskFingerprint.hasByteOrderMark)
        await writes.release(1)
        try await latest.value()

        XCTAssertEqual(try Data(contentsOf: url), bom + Data(latestSource.utf8))
        XCTAssertFalse(session.isDirty(source: latestSource))
        XCTAssertEqual(session.currentDocumentIdentity, identity)
        XCTAssertFalse(session.isSaving)
        XCTAssertNil(session.attention)
        let allWrites = await writes.requests
        XCTAssertEqual(allWrites.count, 2, "The superseded intermediate source must never be written")
    }

    @MainActor
    func testCancelledWriterThatAlreadyCommitsAdvancesBaselineBeforeQueuedSave() async throws {
        let original = "# Original\n"
        let firstSource = "# Committed despite cancellation\n"
        let latestSource = "# Newest edit\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: original, defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        let first = await MarkdownSaveCall.start(session, source: firstSource)
        _ = await writes.waitForRequest(0)
        first.cancel()
        await writes.waitUntilCancelled(0)
        let latest = await MarkdownSaveCall.start(session, source: latestSource)
        guard assertPending(latest) else { return }

        // The fixture models a provider beyond its final cancellation check:
        // it commits real bytes even after its save Task is cancelled.
        await writes.release(0)
        let nextWrite = await writes.waitForRequest(1)
        try await first.value()
        XCTAssertEqual(nextWrite.baseline.diskFingerprint.digest, Data(SHA256.hash(data: Data(firstSource.utf8))))
        XCTAssertEqual(session.currentDocument?.source, firstSource)
        XCTAssertTrue(session.isSaving)
        await writes.release(1)
        try await latest.value()

        let cancellations = await writes.cancellationAtCommit
        XCTAssertEqual(cancellations[0], true)
        XCTAssertEqual(cancellations[1], false, "The queued save must have a fresh, uncancelled Task context")
        XCTAssertEqual(try Data(contentsOf: url), Data(latestSource.utf8))
        XCTAssertFalse(session.isDirty(source: latestSource))
        XCTAssertNil(session.attention)
    }

    @MainActor
    func testCancelledWriterBeforeCommitStillRunsLatestQueuedSave() async throws {
        let original = "# Original\n"
        let latestSource = "# Latest\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: original, defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        let first = await MarkdownSaveCall.start(session, source: "# Cancelled edit\n")
        _ = await writes.waitForRequest(0)
        let latest = await MarkdownSaveCall.start(session, source: latestSource)
        guard assertPending(latest) else { return }
        first.cancel()
        await writes.waitUntilCancelled(0)
        await writes.fail(0, with: CancellationError())
        guard await assertCancelled(first) else { return }
        let nextWrite = await writes.waitForRequest(1)
        XCTAssertEqual(nextWrite.baseline.source, original)
        await writes.release(1)
        try await latest.value()
        XCTAssertEqual(try Data(contentsOf: url), Data(latestSource.utf8))
        XCTAssertFalse(session.isSaving)
    }

    @MainActor
    func testUndoToOriginalSourceStillAutosavesAfterOlderWriteCommits() async throws {
        let original = "# Original\r\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: original, defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        XCTAssertFalse(session.needsAutosave(source: original))
        let first = await MarkdownSaveCall.start(session, source: "# Edit that the user undoes\r\n")
        _ = await writes.waitForRequest(0)
        XCTAssertFalse(session.isDirty(source: original))
        XCTAssertTrue(session.needsAutosave(source: original), "The active write can still change the baseline after Undo")
        let undone = await MarkdownSaveCall.start(session, source: original)
        guard assertPending(undone) else { return }
        await writes.release(0)
        let nextWrite = await writes.waitForRequest(1)
        try await first.value()
        XCTAssertEqual(nextWrite.source, original)
        await writes.release(1)
        try await undone.value()
        XCTAssertEqual(try Data(contentsOf: url), Data(original.utf8))
        XCTAssertFalse(session.needsAutosave(source: original))
    }

    @MainActor
    func testCancelledQueuedSaveNeverWritesAndFreshSaveRemainsAvailable() async throws {
        let original = "# Original\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: original, defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        let first = await MarkdownSaveCall.start(session, source: "# First\n")
        _ = await writes.waitForRequest(0)
        let queued = await MarkdownSaveCall.start(session, source: "# Cancelled queued edit\n")
        queued.cancel()
        guard await assertCancelled(queued) else { return }
        await writes.release(0)
        try await first.value()
        XCTAssertFalse(session.isSaving)
        let count = await writes.requestCount
        XCTAssertEqual(count, 1)
        XCTAssertEqual(try Data(contentsOf: url), Data("# First\n".utf8))

        let fresh = await MarkdownSaveCall.start(session, source: "# Fresh\n")
        _ = await writes.waitForRequest(1)
        await writes.release(1)
        try await fresh.value()
        XCTAssertEqual(try Data(contentsOf: url), Data("# Fresh\n".utf8))
    }

    @MainActor
    func testQueuedSaveStopsAtExternalConflictAndCanRetryAfterExplicitReopen() async throws {
        let original = "# Original\n"
        let external = "# Changed outside the app\n"
        let latestSource = "# Latest local edit\n"
        let url = try temporarySourceURL(contents: Data(original.utf8))
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: original, defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(try await MarkdownSourceLoader.open(from: url))
        let first = await MarkdownSaveCall.start(session, source: "# First local edit\n")
        _ = await writes.waitForRequest(0)
        let latest = await MarkdownSaveCall.start(session, source: latestSource)
        guard assertPending(latest) else { return }
        try Data(external.utf8).write(to: url, options: .atomic)
        await writes.release(0)
        guard await assertSaveFailure(first, equals: .changedOnDisk),
              await assertSaveFailure(latest, equals: .changedOnDisk) else { return }
        XCTAssertEqual(try Data(contentsOf: url), Data(external.utf8))
        XCTAssertEqual(session.attention, .changedOnDisk)
        XCTAssertTrue(session.isDirty(source: latestSource))
        XCTAssertFalse(session.isSaving)
        let count = await writes.requestCount
        XCTAssertEqual(count, 1, "A conflict must not trigger an automatic queued retry")

        session.adopt(try await MarkdownSourceLoader.open(from: url))
        let retry = await MarkdownSaveCall.start(session, source: latestSource)
        _ = await writes.waitForRequest(1)
        await writes.release(1)
        try await retry.value()
        XCTAssertEqual(try Data(contentsOf: url), Data(latestSource.utf8))
        XCTAssertNil(session.attention)
    }

    @MainActor
    func testOpeningAnotherDocumentRetiresQueuedSourceAndKeepsNewSaveOwnership() async throws {
        let firstDocument = try await testDocument("# First\n", name: "first.md")
        let secondDocument = try await testDocument("# Second\n", name: "second.md")
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n", defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(firstDocument)
        let first = await MarkdownSaveCall.start(session, source: "# First committed\n")
        _ = await writes.waitForRequest(0)
        let oldQueued = await MarkdownSaveCall.start(session, source: "# Old queued edit\n")
        guard assertPending(oldQueued) else { return }
        session.adopt(secondDocument)
        let secondIdentity = session.currentDocumentIdentity
        guard await assertCancelled(oldQueued) else { return }
        let second = await MarkdownSaveCall.start(session, source: "# Second committed\n")
        _ = await writes.waitForRequest(1)

        await writes.release(0)
        try await first.value()
        XCTAssertEqual(session.currentDocument, secondDocument)
        XCTAssertEqual(session.currentDocumentIdentity, secondIdentity)
        XCTAssertTrue(session.isSaving)
        XCTAssertEqual(session.recentDocuments.first?.displayName, "second.md")
        await writes.release(1)
        try await second.value()

        XCTAssertEqual(try Data(contentsOf: firstDocument.url), Data("# First committed\n".utf8))
        XCTAssertEqual(try Data(contentsOf: secondDocument.url), Data("# Second committed\n".utf8))
        let allWrites = await writes.requests
        XCTAssertEqual(allWrites.map(\.source), ["# First committed\n", "# Second committed\n"])
        XCTAssertFalse(session.isSaving)
    }

    @MainActor
    func testNewUntitledDocumentCannotReceivePreviousDocumentsQueuedSave() async throws {
        let document = try await testDocument("# Original\n")
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n", defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(document)
        let first = await MarkdownSaveCall.start(session, source: "# Previous write\n")
        _ = await writes.waitForRequest(0)
        let queued = await MarkdownSaveCall.start(session, source: "# Previous queued edit\n")
        guard assertPending(queued) else { return }
        session.beginUntitled(source: "# New document\n")
        guard await assertCancelled(queued) else { return }
        await writes.release(0)
        try await first.value()
        XCTAssertFalse(session.hasCurrentDocument)
        XCTAssertNil(session.currentDocumentIdentity)
        XCTAssertFalse(session.isDirty(source: "# New document\n"))
        XCTAssertFalse(session.isSaving)
        let count = await writes.requestCount
        XCTAssertEqual(count, 1)
        XCTAssertEqual(try Data(contentsOf: document.url), Data("# Previous write\n".utf8))
    }

    @MainActor
    func testCancelledTemplateRequestPreservesOwnedFileAndUnsavedEdits() async throws {
        let defaults = try makeDefaults()
        let document = try await testDocument("# Original file\n")
        let session = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        session.adopt(document)
        let identity = session.currentDocumentIdentity
        let generation = session.documentGeneration
        let reference = defaults.data(forKey: MarkdownDocumentSession.activeDocumentStorageKey)
        let edits = "# Keep these unsaved edits\n"
        let request = session.prepareUntitled(source: "# Template\n", title: "Template")

        XCTAssertEqual(session.currentDocument, document, "Selecting a template must not detach before confirmation")
        XCTAssertEqual(session.documentGeneration, generation)
        XCTAssertTrue(session.isDirty(source: edits))
        session.cancelDocumentRequest(request.id)
        XCTAssertFalse(session.beginUntitled(request), "A late confirmation cannot revive a cancelled template")
        XCTAssertEqual(session.currentDocument, document)
        XCTAssertEqual(session.currentDocumentIdentity, identity)
        XCTAssertTrue(session.isDirty(source: edits))
        XCTAssertEqual(defaults.data(forKey: MarkdownDocumentSession.activeDocumentStorageKey), reference)
        XCTAssertEqual(try Data(contentsOf: document.url), Data(document.source.utf8))
    }

    @MainActor
    func testOnlyNewestTemplateCanDetachTheFileAndItCannotAutosaveIntoTheOriginal() async throws {
        let defaults = try makeDefaults()
        let document = try await testDocument("# Keep this file\n")
        let session = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: defaults)
        session.adopt(document)
        let first = session.prepareUntitled(source: "# Earlier template\n", title: "Earlier")
        let latest = session.prepareUntitled(source: "# Selected template 🧪\n", title: "Selected")
        session.cancelDocumentRequest(first.id)

        XCTAssertFalse(session.beginUntitled(first))
        XCTAssertTrue(session.beginUntitled(latest), "Dismissing an older alert cannot cancel the newer choice")
        XCTAssertFalse(session.beginUntitled(latest), "A request is consumed by the document transition")
        XCTAssertFalse(session.hasCurrentDocument)
        XCTAssertNil(session.currentDocumentIdentity)
        XCTAssertNil(defaults.data(forKey: MarkdownDocumentSession.activeDocumentStorageKey))
        XCTAssertEqual(session.displayName, "Untitled.md")
        XCTAssertFalse(session.isDirty(source: latest.source))
        XCTAssertFalse(session.needsAutosave(source: latest.source))
        XCTAssertFalse(session.needsAutosave(source: latest.source + "More edits\n"))
        XCTAssertEqual(session.recentDocuments.first?.displayName, document.displayName)
        do {
            try await session.save(source: latest.source)
            XCTFail("An untitled template must require a new save destination")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.DocumentError, .noCurrentDocument)
        }
        XCTAssertEqual(try Data(contentsOf: document.url), Data(document.source.utf8))
    }

    @MainActor
    func testFileOpenSupersedesTemplateWaitingForLabDismissalOrConfirmation() async throws {
        let first = try await testDocument("# First file\n", name: "first.md")
        let selected = try await testDocument("# Selected file\n", name: "selected.md")
        let session = MarkdownDocumentSession(initialSource: "# Untitled\n", defaults: try makeDefaults())
        session.adopt(first)
        let template = session.prepareUntitled(source: "# Obsolete template\n", title: "Obsolete")
        let openRequest = session.beginDocumentRequest()

        XCTAssertFalse(session.beginUntitled(template), "Even a still-reading Open owns the newer intent")
        session.cancelDocumentRequest(template.id)
        let opened = try await session.open(from: selected.url, request: openRequest)
        session.adopt(try XCTUnwrap(opened))
        XCTAssertFalse(session.beginUntitled(template))
        XCTAssertEqual(session.currentDocument?.url, selected.url)
        XCTAssertFalse(session.isDirty(source: selected.source))
        XCTAssertEqual(try Data(contentsOf: first.url), Data(first.source.utf8))
    }

    @MainActor
    func testConfirmedTemplateRetiresOldAutosaveQueueWithoutReattachingItsFile() async throws {
        let document = try await testDocument("# Original\n")
        let writes = SuspendedMarkdownSaves()
        defer { Task { await writes.releaseAll() } }
        let session = MarkdownDocumentSession(
            initialSource: "# Untitled\n", defaults: try makeDefaults(),
            saveDocument: { try await writes.save($0, replacing: $1) }
        )
        session.adopt(document)
        let first = await MarkdownSaveCall.start(session, source: "# Already saving original edits\n")
        _ = await writes.waitForRequest(0)
        let queued = await MarkdownSaveCall.start(session, source: "# Superseded original edits\n")
        let template = session.prepareUntitled(source: "# New template\n", title: "Template")
        guard assertPending(queued) else { return }
        XCTAssertTrue(session.hasCurrentDocument, "Opening the confirmation must preserve the live file")
        XCTAssertTrue(session.beginUntitled(template))
        guard await assertCancelled(queued) else { return }
        XCTAssertFalse(session.needsAutosave(source: template.source))

        // The provider may already have committed valid old edits. Its late
        // reply must not reconnect the template to that file or drain its queue.
        await writes.release(0)
        try await first.value()
        XCTAssertFalse(session.hasCurrentDocument)
        XCTAssertNil(session.currentDocumentIdentity)
        XCTAssertFalse(session.isSaving)
        XCTAssertFalse(session.isDirty(source: template.source))
        let allWrites = await writes.requests
        XCTAssertEqual(allWrites.map(\.source), ["# Already saving original edits\n"])
        XCTAssertEqual(try Data(contentsOf: document.url), Data("# Already saving original edits\n".utf8))
    }

    @MainActor
    func testOlderAutosaveFinalizerCannotClearOrCancelNewerTask() async throws {
        let autosave = MarkdownDocumentAutosave()
        let gates = SuspendedMarkdownIO()
        let firstDocument = try await testDocument("# First\n", name: "first.md")
        let secondDocument = try await testDocument("# Second\n", name: "second.md")
        var failures = 0
        var secondWasCancelled = false
        let first = autosave.schedule {
            _ = try await gates.read(firstDocument.url)
        } onFailure: { _ in failures += 1 }
        await gates.waitUntilRequested(firstDocument.url)
        let second = autosave.schedule {
            _ = try await gates.read(secondDocument.url)
            secondWasCancelled = Task.isCancelled
        } onFailure: { _ in failures += 1 }
        await gates.waitUntilRequested(secondDocument.url)
        await gates.complete(firstDocument.url, with: .success(firstDocument))
        await first.value

        XCTAssertTrue(autosave.isPending, "Old completion cannot lose the handle needed to cancel a newer save")
        autosave.cancel()
        XCTAssertFalse(autosave.isPending)
        await gates.complete(secondDocument.url, with: .success(secondDocument))
        await second.value
        XCTAssertTrue(secondWasCancelled)
        XCTAssertEqual(failures, 0)
    }

    @MainActor
    func testOlderAutosaveFailureCannotReplaceNewerErrorOrRetireItsTask() async throws {
        let autosave = MarkdownDocumentAutosave()
        let gates = SuspendedMarkdownIO()
        let firstDocument = try await testDocument("# First\n", name: "first.md")
        let secondDocument = try await testDocument("# Second\n", name: "second.md")
        var failures: [MarkdownSourceLoader.DocumentError?] = []
        let first = autosave.schedule {
            _ = try await gates.read(firstDocument.url)
        } onFailure: { failures.append($0 as? MarkdownSourceLoader.DocumentError) }
        await gates.waitUntilRequested(firstDocument.url)
        let second = autosave.schedule {
            _ = try await gates.read(secondDocument.url)
        } onFailure: { failures.append($0 as? MarkdownSourceLoader.DocumentError) }
        await gates.waitUntilRequested(secondDocument.url)
        await gates.complete(firstDocument.url, with: .failure(MarkdownSourceLoader.DocumentError.changedOnDisk))
        await first.value
        XCTAssertTrue(autosave.isPending)
        XCTAssertTrue(failures.isEmpty)
        await gates.complete(secondDocument.url, with: .failure(MarkdownSourceLoader.DocumentError.coordinatedWrite))
        await second.value
        XCTAssertEqual(failures, [.coordinatedWrite])
        XCTAssertFalse(autosave.isPending)
    }

    @MainActor
    private func assertPending(
        _ call: MarkdownSaveCall, file: StaticString = #filePath, line: UInt = #line
    ) -> Bool {
        XCTAssertFalse(call.isFinished, "A queued save cannot report success before writing its source", file: file, line: line)
        return !call.isFinished
    }

    @MainActor
    func testSceneSessionsRestoreTheirOwnFilesAndMergeSharedRecents() async throws {
        let defaults = try makeDefaults()
        let directory = recoveryDirectory()
        let first = try await MarkdownDraftStore.prepareSceneRecovery(for: MarkdownSceneIdentity(), directory: directory)
        let second = try await MarkdownDraftStore.prepareSceneRecovery(for: MarkdownSceneIdentity(), directory: directory)
        // Create both sessions before opening either file. Their cached recents
        // begin identically and must not overwrite one another's later entries.
        let a = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: first)
        let b = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: second)
        let fileA = try await testDocument("# First\r\n", name: "first.md")
        let fileB = try await testDocument("# Second\n", name: "second.md")
        a.adopt(fileA)
        b.adopt(fileB)
        a.adopt(fileA)
        b.refreshRecents()
        XCTAssertEqual(a.recentDocuments.map(\.displayName), ["first.md", "second.md"])
        XCTAssertEqual(b.recentDocuments, a.recentDocuments)
        XCTAssertNil(defaults.data(forKey: MarkdownDocumentSession.activeDocumentStorageKey))

        let restoredA = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: first)
        let restoredB = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: second)
        let resultA = try await restoredA.restoreActiveDocument(recoveredSource: nil, recoveredDocumentIdentity: nil)
        let resultB = try await restoredB.restoreActiveDocument(recoveredSource: nil, recoveredDocumentIdentity: nil)
        guard case .fileVersion(let openedA) = resultA, case .fileVersion(let openedB) = resultB else {
            return XCTFail("Each scene must restore its own file")
        }
        XCTAssertEqual(openedA.document.url, fileA.url)
        XCTAssertEqual(openedB.document.url, fileB.url)
        XCTAssertEqual(openedA.documentIdentity, a.currentDocumentIdentity)
        XCTAssertEqual(openedB.documentIdentity, b.currentDocumentIdentity)
    }

    @MainActor
    func testLegacyBookmarkHasOneSceneOwnerAndNewCannotResurrectIt() async throws {
        let defaults = try makeDefaults()
        let file = try await testDocument("# Before scene namespaces\n", name: "legacy.md")
        let legacy = MarkdownDocumentSession(initialSource: "", defaults: defaults)
        legacy.adopt(file)
        let legacyReference = try XCTUnwrap(defaults.data(forKey: MarkdownDocumentSession.activeDocumentStorageKey))
        let directory = recoveryDirectory()
        let firstID = MarkdownSceneIdentity()
        let first = try await MarkdownDraftStore.prepareSceneRecovery(for: firstID, directory: directory)
        let second = try await MarkdownDraftStore.prepareSceneRecovery(for: MarkdownSceneIdentity(), directory: directory)
        let a = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: first)
        let b = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: second)
        let resultA = try await a.restoreActiveDocument(recoveredSource: nil, recoveredDocumentIdentity: nil)
        let resultB = try await b.restoreActiveDocument(recoveredSource: nil, recoveredDocumentIdentity: nil)
        guard case .fileVersion(let restored) = resultA else {
            return XCTFail("The migration owner must retain the old bookmark")
        }
        XCTAssertEqual(restored.document.url, file.url)
        XCTAssertEqual(resultB, .none)

        a.beginUntitled(source: "# New draft")
        let preparedAgain = try await MarkdownDraftStore.prepareSceneRecovery(for: firstID, directory: directory)
        let relaunched = MarkdownDocumentSession(initialSource: "# New draft", defaults: defaults, sceneRecovery: preparedAgain)
        let afterNew = try await relaunched.restoreActiveDocument(recoveredSource: nil, recoveredDocumentIdentity: nil)
        XCTAssertEqual(afterNew, .none, "A cleared scene bookmark is not a request to replay legacy state")
        XCTAssertEqual(defaults.data(forKey: MarkdownDocumentSession.activeDocumentStorageKey), legacyReference)
    }

    @MainActor
    func testInFlightSavesAndNewInOneSceneCannotReplaceAnotherScenesRestoration() async throws {
        let defaults = try makeDefaults()
        let directory = recoveryDirectory()
        let first = try await MarkdownDraftStore.prepareSceneRecovery(for: MarkdownSceneIdentity(), directory: directory)
        let second = try await MarkdownDraftStore.prepareSceneRecovery(for: MarkdownSceneIdentity(), directory: directory)
        let savesA = SuspendedMarkdownSaves()
        let savesB = SuspendedMarkdownSaves()
        addTeardownBlock {
            await savesA.releaseAll()
            await savesB.releaseAll()
        }
        let a = MarkdownDocumentSession(
            initialSource: "", defaults: defaults, sceneRecovery: first,
            saveDocument: { try await savesA.save($0, replacing: $1) }
        )
        let b = MarkdownDocumentSession(
            initialSource: "", defaults: defaults, sceneRecovery: second,
            saveDocument: { try await savesB.save($0, replacing: $1) }
        )
        let fileA = try await testDocument("# First original\n", name: "first.md")
        let fileB = try await testDocument("# Second original\n", name: "second.md")
        a.adopt(fileA)
        b.adopt(fileB)
        let callA = await MarkdownSaveCall.start(a, source: "# First changed\n")
        _ = await savesA.waitForRequest(0)
        let callB = await MarkdownSaveCall.start(b, source: "# Second changed\n")
        _ = await savesB.waitForRequest(0)
        a.beginUntitled(source: "# Different untitled source")
        await savesB.release(0)
        try await callB.value()
        await savesA.release(0)
        guard await assertCancelled(callA) else { return }

        let restoredA = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: first)
        let restoredB = MarkdownDocumentSession(initialSource: "", defaults: defaults, sceneRecovery: second)
        let resultA = try await restoredA.restoreActiveDocument(recoveredSource: nil, recoveredDocumentIdentity: nil)
        let resultB = try await restoredB.restoreActiveDocument(recoveredSource: nil, recoveredDocumentIdentity: nil)
        XCTAssertEqual(resultA, .none)
        guard case .fileVersion(let openedB) = resultB else {
            return XCTFail("The independently saved second document must still restore")
        }
        XCTAssertEqual(openedB.document.url, fileB.url)
        XCTAssertEqual(openedB.document.source, "# Second changed\n")
        XCTAssertEqual(openedB.documentIdentity, b.currentDocumentIdentity)
    }

    private func recoveryDirectory() -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("frankenmarkdown-scenes-\(UUID().uuidString)", isDirectory: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        return directory
    }

    @MainActor
    private func assertCancelled(
        _ call: MarkdownSaveCall, file: StaticString = #filePath, line: UInt = #line
    ) async -> Bool {
        do {
            try await call.value()
            XCTFail("A cancelled or superseded save must not report success", file: file, line: line)
        } catch is CancellationError {
            return true
        } catch {
            XCTFail("Expected cancellation, received \(error)", file: file, line: line)
        }
        return false
    }

    @MainActor
    private func assertSaveFailure(
        _ call: MarkdownSaveCall,
        equals expected: MarkdownSourceLoader.DocumentError,
        file: StaticString = #filePath, line: UInt = #line
    ) async -> Bool {
        do {
            try await call.value()
            XCTFail("The queued caller must receive the save failure", file: file, line: line)
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.DocumentError, expected, file: file, line: line)
            return error as? MarkdownSourceLoader.DocumentError == expected
        }
        return false
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

/// The main-actor entry prefix reserves the save before yielding to its caller.
/// Tests can inspect completion without sleeps, timeouts, or provider-speed assumptions.
@MainActor
private final class MarkdownSaveCall {
    private var task: Task<Void, Error>!
    private(set) var isFinished = false

    static func start(_ session: MarkdownDocumentSession, source: String) async -> MarkdownSaveCall {
        let call = MarkdownSaveCall()
        await withCheckedContinuation { entered in
            call.task = Task { @MainActor in
                defer { call.isFinished = true }
                entered.resume()
                try await session.save(source: source)
            }
        }
        return call
    }

    func cancel() { task.cancel() }
    func value() async throws { try await task.value }
}

/// Gate actual coordinated writes and record the baseline each write received.
/// A released write runs in an independent task to model the legitimate case
/// where a provider has passed its last cancellation check and still commits.
private actor SuspendedMarkdownSaves {
    struct Request: Sendable {
        let source: String
        let baseline: MarkdownSourceDocument
    }

    private(set) var requests: [Request] = []
    private(set) var cancellationAtCommit: [Int: Bool] = [:]
    private var gates: [Int: CheckedContinuation<Void, Error>] = [:]
    private var observers: [Int: CheckedContinuation<Request, Never>] = [:]
    private var cancelled: Set<Int> = []
    private var cancellationObservers: [Int: CheckedContinuation<Void, Never>] = [:]
    private var unblocked = false
    var requestCount: Int { requests.count }

    func save(_ source: String, replacing document: MarkdownSourceDocument) async throws -> MarkdownSourceDocument {
        let index = requests.count
        let request = Request(source: source, baseline: document)
        requests.append(request)
        if !unblocked {
            try await withTaskCancellationHandler {
                try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                    gates[index] = continuation
                    observers.removeValue(forKey: index)?.resume(returning: request)
                }
            } onCancel: {
                Task { await self.didCancel(index) }
            }
        }
        cancellationAtCommit[index] = Task.isCancelled
        let writer = Task.detached {
            try await MarkdownSourceLoader.save(source, replacing: document)
        }
        return try await writer.value
    }

    func waitForRequest(_ index: Int) async -> Request {
        if index < requests.count { return requests[index] }
        return await withCheckedContinuation { observers[index] = $0 }
    }

    func release(_ index: Int) { gates.removeValue(forKey: index)?.resume() }
    func fail(_ index: Int, with error: Error) { gates.removeValue(forKey: index)?.resume(throwing: error) }

    private func didCancel(_ index: Int) {
        cancelled.insert(index)
        cancellationObservers.removeValue(forKey: index)?.resume()
    }

    func waitUntilCancelled(_ index: Int) async {
        if cancelled.contains(index) { return }
        await withCheckedContinuation { cancellationObservers[index] = $0 }
    }

    func releaseAll() {
        unblocked = true
        let remaining = Array(gates.values)
        gates.removeAll()
        for continuation in remaining { continuation.resume() }
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
