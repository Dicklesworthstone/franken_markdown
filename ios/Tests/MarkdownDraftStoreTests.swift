import CryptoKit
import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownDraftStoreTests: XCTestCase {
    func testRoundTripPreservesDocumentAndSafeSettings() async throws {
        let store = makeStore()
        let draft = makeDraft(source: "# Recovered\n\nPrivate words.")

        try await store.save(draft)

        let loaded = await store.load()
        XCTAssertEqual(loaded, draft)
    }

    func testLargeEscapedUnicodeDraftUsesAFileInsteadOfJSONBody() async throws {
        let store = makeStore()
        let source = "\u{FEFF}"
            + String(repeating: "\"\\\nCafé 👩🏽‍💻\r\n", count: 350_000)
            + "FINAL-TAIL"
        let draft = makeDraft(source: source, documentIdentity: UUID())
        XCTAssertGreaterThan(source.utf8.count, MarkdownDraftStore.maximumLegacySourceBytes)

        try await store.save(draft)

        let metadata = try Data(contentsOf: store.fileURL)
        XCTAssertLessThan(metadata.count, 4 * 1_024)
        let manifest = try readManifest(store)
        let settings = try XCTUnwrap(manifest["draft"] as? [String: Any])
        XCTAssertEqual(settings["source"] as? String, "")
        XCTAssertEqual(manifest["sourceByteCount"] as? Int, source.utf8.count)
        let sourceURL = try storedSourceURL(store)
        let size = try sourceURL.resourceValues(forKeys: [.fileSizeKey]).fileSize
        XCTAssertEqual(size, source.utf8.count)
        let loaded = await store.load()
        let recovered = try XCTUnwrap(loaded)
        XCTAssertEqual(recovered, draft)
        XCTAssertTrue(recovered.source.utf8.elementsEqual(source.utf8))
        XCTAssertTrue(recovered.source.hasSuffix("FINAL-TAIL"))
    }

    func testEmptyAndLiteralLeadingBOMContentRoundTripExactly() async throws {
        let store = makeStore()
        for source in ["", "\u{FEFF}", "\u{FEFF}\u{FEFF}# Literal marks\r\nInterior \u{FEFF}"] {
            let draft = makeDraft(source: source)
            try await store.save(draft)
            let sourceURL = try storedSourceURL(store)
            XCTAssertEqual(try Data(contentsOf: sourceURL), Data(source.utf8))
            let loaded = await store.load()
            let recovered = try XCTUnwrap(loaded)
            XCTAssertTrue(recovered.source.utf8.elementsEqual(source.utf8))
        }
    }

    func testMalformedDraftFailsClosed() async throws {
        let store = makeStore()
        try createParent(of: store.fileURL)
        try Data("not-json".utf8).write(to: store.fileURL, options: .atomic)

        let loaded = await store.load()
        XCTAssertNil(loaded)
    }

    func testOversizedSourceIsRefused() async {
        let store = makeStore()
        let source = String(repeating: "a", count: MarkdownDraftStore.maximumSourceBytes + 1)

        await assertInvalidDraft(makeDraft(source: source), store: store)
        XCTAssertFalse(FileManager.default.fileExists(atPath: store.fileURL.path))
    }

    func testUnsafeRestoredRendererSettingIsRefused() async {
        let store = makeStore()
        let draft = makeDraft(source: "# Safe", fontFamily: "remote-font")

        await assertInvalidDraft(draft, store: store)
    }

    func testSchemaOneDraftWithoutPDFTypographyStillLoadsAndMigratesOnSave() async throws {
        let store = makeStore()
        try createParent(of: store.fileURL)
        let legacy = """
        {
          "schema": 1,
          "savedAtMilliseconds": 1725350400000,
          "source": "# Existing draft",
          "title": "Existing",
          "author": "Author",
          "fontFamily": "sans",
          "rendererDarkMode": "auto",
          "tableOfContents": false,
          "tableOfContentsDepth": 3,
          "pageNumbers": false,
          "codeLineNumbers": false,
          "language": "en",
          "microtypeProtrusion": false,
          "fitToPages": 0
        }
        """
        try Data(legacy.utf8).write(to: store.fileURL, options: .atomic)

        let loaded = await store.load()
        let restored = try XCTUnwrap(loaded)
        XCTAssertEqual(restored.source, "# Existing draft")
        XCTAssertNil(restored.customizePDFTypography)
        XCTAssertNil(restored.pdfBaseFontSize)
        try await store.save(restored)
        let manifest = try readManifest(store)
        XCTAssertEqual(manifest["storageSchema"] as? Int, 2)
        let migrated = await store.load()
        XCTAssertEqual(migrated, restored)
    }

    func testLegacyJSONRetainsItsOriginalSourceLimit() async throws {
        let store = makeStore()
        try createParent(of: store.fileURL)
        let draft = makeDraft(
            source: String(repeating: "a", count: MarkdownDraftStore.maximumLegacySourceBytes + 1)
        )
        let data = try JSONEncoder().encode(draft)
        XCTAssertLessThan(data.count, MarkdownDraftStore.maximumLegacyEncodedBytes)
        try data.write(to: store.fileURL, options: .atomic)

        let legacy = await store.load()
        XCTAssertNil(legacy)
        // The identical source is supported through the bounded new format.
        try await store.save(draft)
        let migrated = await store.load()
        XCTAssertEqual(migrated, draft)
    }

    func testDocumentIdentityRoundTripsWithRecoveredDraft() async throws {
        let store = makeStore()
        let documentIdentity = UUID()

        try await store.save(makeDraft(source: "# Identified draft", documentIdentity: documentIdentity))

        let loaded = await store.load()
        XCTAssertEqual(loaded?.documentIdentity, documentIdentity)
    }

    func testOversizedCustomStylesheetIsRefused() async {
        let store = makeStore()
        let draft = makeDraft(
            source: "# Styled",
            customCSS: String(repeating: "x", count: MarkdownActiveDraft.maximumCustomCSSBytes + 1)
        )

        await assertInvalidDraft(draft, store: store)
    }

    func testMetadataBudgetAllowsWorstCaseEscapedStylesheet() async throws {
        let store = makeStore()
        let draft = makeDraft(
            source: "# Styled",
            customCSS: String(repeating: "\0", count: MarkdownActiveDraft.maximumCustomCSSBytes)
        )

        try await store.save(draft)

        let size = try store.fileURL.resourceValues(forKeys: [.fileSizeKey]).fileSize
        XCTAssertLessThanOrEqual(try XCTUnwrap(size), MarkdownDraftStore.maximumMetadataBytes)
        let loaded = await store.load()
        XCTAssertEqual(loaded, draft)
    }

    func testSameSizeCorruptionAndTruncationAreRejected() async throws {
        let store = makeStore()
        let draft = makeDraft(
            source: String(repeating: "A complete chunk.\n", count: 10_000) + "ORIGINAL-TAIL"
        )
        try await store.save(draft)
        let sourceURL = try storedSourceURL(store)
        let corrupt = try FileHandle(forWritingTo: sourceURL)
        try corrupt.seek(toOffset: UInt64(draft.source.utf8.count - 1))
        try corrupt.write(contentsOf: Data("!".utf8))
        try corrupt.close()

        let corrupted = await store.load()
        XCTAssertNil(corrupted)

        try await store.save(draft)
        let nextSourceURL = try storedSourceURL(store)
        let truncated = try FileHandle(forWritingTo: nextSourceURL)
        try truncated.truncate(atOffset: UInt64(draft.source.utf8.count - 1))
        try truncated.close()

        let incomplete = await store.load()
        XCTAssertNil(incomplete)
    }

    func testMalformedUTF8IsRejectedEvenWhenItsDigestMatches() async throws {
        let store = makeStore()
        try await store.save(makeDraft(source: "valid"))
        let bytes = Data([0xC3, 0x28])
        try bytes.write(to: storedSourceURL(store), options: .atomic)
        var manifest = try readManifest(store)
        manifest["sourceByteCount"] = bytes.count
        manifest["sourceSHA256"] = Data(SHA256.hash(data: bytes)).base64EncodedString()
        try writeManifest(manifest, to: store)

        let loaded = await store.load()
        XCTAssertNil(loaded)
    }

    func testManifestRejectsUnsafeSourceNamesAndInvalidIntegrityFields() async throws {
        let store = makeStore()
        let source = "# Protected"
        try await store.save(makeDraft(source: source))
        let original = try readManifest(store)
        let lowercaseName = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa.utf8"
        let alternateExtension = UUID().uuidString + ".txt"
        // Every alternate file contains the expected bytes and hash. A loader
        // that merely opens the supplied path would incorrectly recover it.
        for alternate in [lowercaseName, "manual.utf8", alternateExtension] {
            try Data(source.utf8).write(to: sourceDirectory(store).appendingPathComponent(alternate))
        }
        let outside = store.fileURL.deletingLastPathComponent().appendingPathComponent("outside.md")
        try Data(source.utf8).write(to: outside)
        let invalidNames = [
            "../outside.md", lowercaseName, "manual.utf8", alternateExtension
        ]
        for name in invalidNames {
            var manifest = original
            manifest["sourceFileName"] = name
            try writeManifest(manifest, to: store)
            let loaded = await store.load()
            XCTAssertNil(loaded, name)
        }
        for count in [-1, MarkdownDraftStore.maximumSourceBytes + 1] {
            var manifest = original
            manifest["sourceByteCount"] = count
            try writeManifest(manifest, to: store)
            let loaded = await store.load()
            XCTAssertNil(loaded)
        }
        var invalidDigest = original
        invalidDigest["sourceSHA256"] = Data([0]).base64EncodedString()
        try writeManifest(invalidDigest, to: store)
        let loaded = await store.load()
        XCTAssertNil(loaded)
    }

    func testSourceSymlinkCannotRedirectRecoveryOutsideItsDirectory() async throws {
        let store = makeStore()
        let draft = makeDraft(source: "# Protected")
        try await store.save(draft)
        let outside = store.fileURL.deletingLastPathComponent().appendingPathComponent("outside.md")
        try Data(draft.source.utf8).write(to: outside)
        let name = UUID().uuidString + ".utf8"
        let link = sourceDirectory(store).appendingPathComponent(name)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: outside)
        var manifest = try readManifest(store)
        manifest["sourceFileName"] = name
        try writeManifest(manifest, to: store)

        let loaded = await store.load()
        XCTAssertNil(loaded)
    }

    func testUnknownStorageSchemaDoesNotFallBackToLegacyJSON() async throws {
        let store = makeStore()
        try createParent(of: store.fileURL)
        let legacyData = try JSONEncoder().encode(makeDraft(source: "# Legacy-looking"))
        var metadata = try XCTUnwrap(
            JSONSerialization.jsonObject(with: legacyData) as? [String: Any]
        )
        metadata["storageSchema"] = 999
        try writeManifest(metadata, to: store)

        let loaded = await store.load()
        XCTAssertNil(loaded)
    }

    func testOversizedMetadataIsRefused() async throws {
        let store = makeStore()
        try createParent(of: store.fileURL)
        try Data("{".utf8).write(to: store.fileURL)
        let handle = try FileHandle(forWritingTo: store.fileURL)
        try handle.truncate(atOffset: UInt64(MarkdownDraftStore.maximumLegacyEncodedBytes + 1))
        try handle.close()

        let loaded = await store.load()
        XCTAssertNil(loaded)
    }

    func testFailedPublicationKeepsPriorSnapshotAndRemovesUnreferencedSource() async throws {
        let store = makeStore()
        let previous = makeDraft(source: "# Last durable draft", documentIdentity: UUID())
        try await store.save(previous)
        let metadata = try Data(contentsOf: store.fileURL)
        let sourceURL = try storedSourceURL(store)
        let failing = MarkdownDraftStore(fileURL: store.fileURL, beforePublishing: {
            throw CocoaError(.fileWriteOutOfSpace)
        })
        do {
            try await failing.save(makeDraft(source: String(repeating: "New text.\n", count: 30_000)))
            XCTFail("A failed publication must not report success")
        } catch {
            XCTAssertEqual((error as? CocoaError)?.code, .fileWriteOutOfSpace)
        }

        XCTAssertEqual(try Data(contentsOf: store.fileURL), metadata)
        XCTAssertTrue(FileManager.default.fileExists(atPath: sourceURL.path))
        XCTAssertEqual(try sourceFiles(store).count, 1)
        let loaded = await store.load()
        XCTAssertEqual(loaded, previous)
    }

    func testCancellationAfterWritingSourcePreservesPriorSnapshot() async throws {
        let store = makeStore()
        let previous = makeDraft(source: "# Last durable draft", documentIdentity: UUID())
        try await store.save(previous)
        let metadata = try Data(contentsOf: store.fileURL)
        let cancelling = MarkdownDraftStore(fileURL: store.fileURL, beforePublishing: {
            withUnsafeCurrentTask { $0?.cancel() }
        })
        let replacement = makeDraft(source: String(repeating: "Unpublished text.\n", count: 30_000))
        let worker = Task.detached {
            try await cancelling.save(replacement)
        }
        do {
            try await worker.value
            XCTFail("Cancellation before publication must preserve the old manifest")
        } catch {
            XCTAssertTrue(error is CancellationError)
        }

        XCTAssertEqual(try Data(contentsOf: store.fileURL), metadata)
        XCTAssertEqual(try sourceFiles(store).count, 1)
        let loaded = await store.load()
        XCTAssertEqual(loaded, previous)
    }

    func testAlreadyCancelledSaveDoesNotCreateRecoveryFiles() async throws {
        let store = makeStore()
        let draft = makeDraft(source: "# Cancelled")
        let worker = Task.detached {
            withUnsafeCurrentTask { $0?.cancel() }
            try await store.save(draft)
        }
        do {
            try await worker.value
            XCTFail("An already cancelled save must not access storage")
        } catch {
            XCTAssertTrue(error is CancellationError)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: store.fileURL.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: sourceDirectory(store).path))
    }

    func testSuccessfulSaveRemovesOldAndInterruptedGenerationsOnly() async throws {
        let store = makeStore()
        try await store.save(makeDraft(source: "# Old"))
        let previous = try storedSourceURL(store)
        let orphan = sourceDirectory(store).appendingPathComponent(UUID().uuidString + ".utf8")
        let unrelated = sourceDirectory(store).appendingPathComponent("notes.txt")
        try Data("Interrupted source".utf8).write(to: orphan)
        try Data("Not owned by recovery".utf8).write(to: unrelated)

        let draft = makeDraft(source: "# New")
        try await store.save(draft)

        XCTAssertFalse(FileManager.default.fileExists(atPath: previous.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: orphan.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: unrelated.path))
        XCTAssertEqual(try sourceFiles(store).count, 1)
        let loaded = await store.load()
        XCTAssertEqual(loaded, draft)
    }

    func testIndependentStoresSerializePublicationAndCleanup() async throws {
        let store = makeStore()
        let drafts = (0..<12).map { index in
            makeDraft(
                source: "# Draft \(index)\n" + String(repeating: "Complete source.\n", count: 10_000),
                documentIdentity: UUID()
            )
        }
        try await store.save(drafts[0])

        try await withThrowingTaskGroup(of: Void.self) { group in
            for draft in drafts {
                let writer = MarkdownDraftStore(fileURL: store.fileURL)
                group.addTask {
                    try await writer.save(draft)
                    guard let loaded = await writer.load(), drafts.contains(loaded) else {
                        throw RecoveryTestError.tornSnapshot
                    }
                }
            }
            try await group.waitForAll()
        }

        let loaded = await MarkdownDraftStore(fileURL: store.fileURL).load()
        XCTAssertTrue(loaded.map { drafts.contains($0) } ?? false)
        XCTAssertEqual(try sourceFiles(store).count, 1)
    }

    private enum RecoveryTestError: Error {
        case tornSnapshot
    }

    private func assertInvalidDraft(_ draft: MarkdownActiveDraft, store: MarkdownDraftStore) async {
        do {
            try await store.save(draft)
            XCTFail("An invalid draft must be refused")
        } catch {
            XCTAssertEqual(error as? MarkdownDraftStore.StoreError, .invalidDraft)
        }
    }

    private func makeStore() -> MarkdownDraftStore {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        return MarkdownDraftStore(fileURL: directory.appendingPathComponent("active-draft.json"))
    }

    private func createParent(of url: URL) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
    }

    private func sourceDirectory(_ store: MarkdownDraftStore) -> URL {
        store.fileURL.deletingLastPathComponent().appendingPathComponent(
            store.fileURL.lastPathComponent + ".sources", isDirectory: true
        )
    }

    private func sourceFiles(_ store: MarkdownDraftStore) throws -> [URL] {
        try FileManager.default.contentsOfDirectory(
            at: sourceDirectory(store), includingPropertiesForKeys: nil
        ).filter { $0.pathExtension == "utf8" }
    }

    private func readManifest(_ store: MarkdownDraftStore) throws -> [String: Any] {
        try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: store.fileURL)) as? [String: Any]
        )
    }

    private func writeManifest(_ manifest: [String: Any], to store: MarkdownDraftStore) throws {
        try JSONSerialization.data(withJSONObject: manifest).write(to: store.fileURL, options: .atomic)
    }

    private func storedSourceURL(_ store: MarkdownDraftStore) throws -> URL {
        let manifest = try readManifest(store)
        let name = try XCTUnwrap(manifest["sourceFileName"] as? String)
        return sourceDirectory(store).appendingPathComponent(name)
    }

    private func makeDraft(
        source: String,
        documentIdentity: UUID? = nil,
        fontFamily: String = "serif",
        customCSS: String? = "body { color: #123456; }"
    ) -> MarkdownActiveDraft {
        MarkdownActiveDraft(
            schema: MarkdownActiveDraft.currentSchema,
            savedAtMilliseconds: 1_725_350_400_000,
            documentIdentity: documentIdentity,
            source: source,
            title: "Recovered",
            author: "Author",
            fontFamily: fontFamily,
            rendererDarkMode: "auto",
            tableOfContents: true,
            tableOfContentsDepth: 4,
            pageNumbers: true,
            codeLineNumbers: true,
            language: "en",
            microtypeProtrusion: true,
            fitToPages: 12,
            customizePDFTypography: true,
            pdfBaseFontSize: 12,
            pdfHeadingScale: 1.25,
            pdfTableFontSize: 9.5,
            customCSS: customCSS
        )
    }
}
