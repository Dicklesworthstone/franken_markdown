import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownComparisonSessionTests: XCTestCase {
    func testSnapshotLoaderPreservesUTF8AndNewlinesUnderTheSameByteLimit() async throws {
        let source = "# Café e\u{301} 👩🏽‍💻\r\n\r\nLiteral \u{FEFF} inside.\r\n"
        let bytes = Data([0xEF, 0xBB, 0xBF]) + Data(source.utf8)
        let url = try file(bytes)
        let snapshot = try await MarkdownSourceLoader.snapshot(from: url, maximumBytes: bytes.count)
        XCTAssertEqual(Data(snapshot.source.utf8), Data(source.utf8))
        XCTAssertEqual(snapshot.diskFingerprint.byteCount, bytes.count)
        XCTAssertTrue(snapshot.diskFingerprint.hasByteOrderMark)
        do {
            _ = try await MarkdownSourceLoader.snapshot(from: url, maximumBytes: bytes.count - 1)
            XCTFail("A comparison snapshot must retain the normal source byte limit")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.ImportError, .tooLarge(maximumBytes: bytes.count - 1))
        }
    }

    func testSnapshotLoaderRejectsMalformedTextAndAcceptsAnEmptyEarlierVersion() async throws {
        let empty = try await MarkdownSourceLoader.snapshot(from: file(Data()))
        XCTAssertEqual(empty.source, "")
        XCTAssertEqual(empty.diskFingerprint.byteCount, 0)
        do {
            _ = try await MarkdownSourceLoader.snapshot(from: file(Data([0xF0, 0x9F])))
            XCTFail("A truncated scalar must not become replacement characters")
        } catch {
            XCTAssertEqual(error as? MarkdownSourceLoader.ImportError, .notUTF8)
        }
    }

    @MainActor
    func testSnapshotLoaderPropagatesCancellation() async throws {
        let url = try file(Data("source".utf8))
        let task = Task { try await MarkdownSourceLoader.snapshot(from: url) }
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("A cancelled snapshot must not be adopted")
        } catch {
            XCTAssertTrue(error is CancellationError)
        }
    }

    @MainActor
    func testEmptyBaselineIsPresentAndReachesTheComparisonUnchanged() async throws {
        let owner = UUID()
        let identity = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 1)
        let session = MarkdownComparisonSession { _ in "" }
        XCTAssertNil(session.baseline)
        let selection = session.beginSelection(owner: owner)
        let imported = try XCTUnwrap(session.importBaseline(from: inputURL, selection: selection) { owner })
        await imported.value
        XCTAssertEqual(session.baseline, "")
        XCTAssertEqual(session.baselineByteCount, 0)
        var captured: [String] = []
        let work = try XCTUnwrap(session.compare(source: "# New content", identity: identity,
                                                currentIdentity: { identity }) { old, current in
            captured = [old, current]
            return self.preview("insertions")
        })
        await work.value
        XCTAssertEqual(captured, ["", "# New content"])
        XCTAssertEqual(session.preview(for: identity)?.html, "insertions")
        XCTAssertFalse(session.isComparing)
    }

    @MainActor
    func testFailedOrCancelledBaselineReplacementKeepsTheLastCompleteComparison() async throws {
        let owner = UUID()
        let identity = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 1)
        let session = MarkdownComparisonSession { _ in throw ComparisonFailure() }
        session.snapshot("keep this baseline", owner: owner)
        let compared = try XCTUnwrap(session.compare(source: "current", identity: identity,
                                                    currentIdentity: { identity }) { _, _ in self.preview("valid") })
        await compared.value
        let request = session.beginSelection(owner: owner)
        let failed = try XCTUnwrap(session.importBaseline(from: inputURL, selection: request) { owner })
        await failed.value
        XCTAssertEqual(session.baseline, "keep this baseline")
        XCTAssertEqual(session.preview(for: identity)?.html, "valid")
        XCTAssertNotNil(session.errorMessage)
        XCTAssertFalse(session.isImporting)

        let cancelled = session.beginSelection(owner: owner)
        session.selectionFailed(NSError(domain: NSCocoaErrorDomain, code: NSUserCancelledError),
                                selection: cancelled, currentOwner: owner)
        XCTAssertEqual(session.baseline, "keep this baseline")
        XCTAssertEqual(session.preview(for: identity)?.html, "valid")
        XCTAssertNil(session.errorMessage)
        XCTAssertFalse(session.isImporting)
    }

    @MainActor
    func testTaskCancellationSuppressesLateProviderAndRendererFailures() async throws {
        let owner = UUID(), importGate = ComparisonGate<String>(), renderGate = ComparisonGate<SemanticDiffPreview>()
        let identity = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 1)
        let session = MarkdownComparisonSession { _ in try await importGate.suspend() }
        session.snapshot("valid baseline", owner: owner)
        let selection = session.beginSelection(owner: owner)
        let imported = try XCTUnwrap(session.importBaseline(from: inputURL, selection: selection) { owner })
        await importGate.waitUntilStarted()
        imported.cancel()
        importGate.fail()
        await imported.value
        XCTAssertEqual(session.baseline, "valid baseline")
        XCTAssertNil(session.errorMessage)
        XCTAssertFalse(session.isImporting)

        let compared = try XCTUnwrap(session.compare(source: "current", identity: identity, currentIdentity: { identity }) { _, _ in
            try await renderGate.suspend()
        })
        await renderGate.waitUntilStarted()
        compared.cancel()
        renderGate.fail()
        await compared.value
        XCTAssertEqual(session.baseline, "valid baseline")
        XCTAssertNil(session.preview(for: identity))
        XCTAssertNil(session.errorMessage)
        XCTAssertFalse(session.isComparing)
    }

    @MainActor
    func testOlderImportSuccessAndFailureCannotRetireTheNewerSelection() async throws {
        for fails in [false, true] {
            let owner = UUID(), oldGate = ComparisonGate<String>(), newGate = ComparisonGate<String>()
            let session = MarkdownComparisonSession { url in
                if url.lastPathComponent == "old.md" { return try await oldGate.suspend() }
                return try await newGate.suspend()
            }
            session.snapshot("original", owner: owner)
            let oldSelection = session.beginSelection(owner: owner)
            let oldWork = try XCTUnwrap(session.importBaseline(
                from: URL(fileURLWithPath: "/old.md"), selection: oldSelection
            ) { owner })
            await oldGate.waitUntilStarted()
            let newSelection = session.beginSelection(owner: owner)
            let newWork = try XCTUnwrap(session.importBaseline(from: inputURL, selection: newSelection) { owner })
            await newGate.waitUntilStarted()
            if fails { oldGate.fail() } else { oldGate.succeed("obsolete") }
            await oldWork.value
            XCTAssertTrue(session.isImporting)
            XCTAssertEqual(session.baseline, "original")
            XCTAssertNil(session.errorMessage)
            newGate.succeed("latest")
            await newWork.value
            XCTAssertEqual(session.baseline, "latest")
            XCTAssertFalse(session.isImporting)
            XCTAssertNil(session.importBaseline(from: inputURL, selection: oldSelection) { owner })
        }
    }

    @MainActor
    func testSnapshotAndDismissalCancelImportsWithoutLosingAValidBaseline() async throws {
        for snapshotCurrent in [false, true] {
            let owner = UUID(), gate = ComparisonGate<String>()
            let session = MarkdownComparisonSession { _ in try await gate.suspend() }
            session.snapshot("original", owner: owner)
            let selection = session.beginSelection(owner: owner)
            let work = try XCTUnwrap(session.importBaseline(from: inputURL, selection: selection) { owner })
            await gate.waitUntilStarted()
            if snapshotCurrent { session.snapshot("current snapshot", owner: owner) }
            else { session.cancelPending() }
            gate.succeed("late file contents")
            await work.value
            XCTAssertEqual(session.baseline, snapshotCurrent ? "current snapshot" : "original")
            XCTAssertFalse(session.isImporting)
            XCTAssertNil(session.errorMessage)
        }
    }

    @MainActor
    func testChangedDocumentRejectsPickerAndReadResultsBeforeObserverDelivery() async throws {
        var owner = UUID()
        let gate = ComparisonGate<String>()
        let session = MarkdownComparisonSession { _ in try await gate.suspend() }
        session.snapshot("old document", owner: owner)
        let picker = session.beginSelection(owner: owner)
        owner = UUID()
        XCTAssertNil(session.importBaseline(from: inputURL, selection: picker) { owner })
        XCTAssertNil(session.baseline)

        session.snapshot("second document", owner: owner)
        let reading = session.beginSelection(owner: owner)
        let work = try XCTUnwrap(session.importBaseline(from: inputURL, selection: reading) { owner })
        await gate.waitUntilStarted()
        owner = UUID()
        gate.fail()
        await work.value
        XCTAssertNil(session.baseline)
        XCTAssertNil(session.errorMessage)
        XCTAssertFalse(session.isImporting)
    }

    @MainActor
    func testEditingTheSameDocumentDoesNotCancelBaselineImport() async throws {
        let owner = UUID(), gate = ComparisonGate<String>()
        let session = MarkdownComparisonSession { _ in try await gate.suspend() }
        let selection = session.beginSelection(owner: owner)
        let work = try XCTUnwrap(session.importBaseline(from: inputURL, selection: selection) { owner })
        await gate.waitUntilStarted()
        session.sourceDidChange(to: .init(documentOwner: owner, revision: 2))
        XCTAssertTrue(session.isImporting)
        gate.succeed("earlier revision")
        await work.value
        XCTAssertEqual(session.baseline, "earlier revision")
    }

    @MainActor
    func testComparisonUsesCapturedUnicodeBytesAndRejectsStaleSuccessOrFailure() async throws {
        for fails in [false, true] {
            let owner = UUID(), gate = ComparisonGate<SemanticDiffPreview>()
            var identity = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 1)
            let capturedIdentity = identity
            var current = "# e\u{301}\r\n"
            let session = MarkdownComparisonSession()
            session.snapshot("Earlier é\r\n", owner: owner)
            var received: [String] = []
            let work = try XCTUnwrap(session.compare(source: current, identity: identity,
                                                    currentIdentity: { identity }) { old, new in
                received = [old, new]
                return try await gate.suspend()
            })
            await gate.waitUntilStarted()
            current = "# é\r\n"
            identity = .init(documentOwner: owner, revision: 2)
            if fails { gate.fail() } else { gate.succeed(preview("obsolete")) }
            await work.value
            XCTAssertEqual(received.map { Data($0.utf8) }, [Data("Earlier é\r\n".utf8), Data("# e\u{301}\r\n".utf8)])
            XCTAssertNotEqual(Data(received[1].utf8), Data(current.utf8))
            XCTAssertNil(session.preview(for: capturedIdentity))
            XCTAssertNil(session.preview(for: identity))
            XCTAssertNil(session.errorMessage)
            XCTAssertEqual(session.baseline, "Earlier é\r\n")
            XCTAssertFalse(session.isComparing)
        }
    }

    @MainActor
    func testOlderComparisonCannotClearOrPublishOverANewerComparison() async throws {
        let owner = UUID(), oldGate = ComparisonGate<SemanticDiffPreview>(), newGate = ComparisonGate<SemanticDiffPreview>()
        var identity = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 1)
        let session = MarkdownComparisonSession()
        session.snapshot("baseline", owner: owner)
        let old = try XCTUnwrap(session.compare(source: "first", identity: identity, currentIdentity: { identity }) { _, _ in
            try await oldGate.suspend()
        })
        await oldGate.waitUntilStarted()
        identity = .init(documentOwner: owner, revision: 2)
        session.sourceDidChange(to: identity)
        let newer = try XCTUnwrap(session.compare(source: "second", identity: identity, currentIdentity: { identity }) { _, _ in
            try await newGate.suspend()
        })
        await newGate.waitUntilStarted()
        oldGate.fail()
        await old.value
        XCTAssertTrue(session.isComparing)
        XCTAssertNil(session.errorMessage)
        newGate.succeed(preview("latest"))
        await newer.value
        XCTAssertEqual(session.preview(for: identity)?.html, "latest")
        XCTAssertFalse(session.isComparing)
    }

    @MainActor
    func testReplacingBaselineOrClosingTheLabRetiresAnActiveComparison() async throws {
        for snapshotCurrent in [false, true] {
            let owner = UUID(), gate = ComparisonGate<SemanticDiffPreview>()
            let identity = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 1)
            let session = MarkdownComparisonSession()
            session.snapshot("original", owner: owner)
            let work = try XCTUnwrap(session.compare(source: "current", identity: identity, currentIdentity: { identity }) { _, _ in
                try await gate.suspend()
            })
            await gate.waitUntilStarted()
            if snapshotCurrent { session.snapshot("replacement", owner: owner) }
            else { session.cancelPending() }
            gate.succeed(preview("obsolete"))
            await work.value
            XCTAssertNil(session.preview(for: identity))
            XCTAssertNil(session.errorMessage)
            XCTAssertFalse(session.isComparing)
            XCTAssertEqual(session.baseline, snapshotCurrent ? "replacement" : "original")
        }
    }

    @MainActor
    func testCompletedPreviewIsHiddenImmediatelyWhenSourceOrDocumentChanges() async throws {
        let owner = UUID()
        let identity = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 1)
        let session = MarkdownComparisonSession()
        session.snapshot("baseline", owner: owner)
        let work = try XCTUnwrap(session.compare(source: "current", identity: identity, currentIdentity: { identity }) { _, _ in
            self.preview("visible")
        })
        await work.value
        XCTAssertNotNil(session.preview(for: identity))
        let edited = MarkdownComparisonSession.SourceIdentity(documentOwner: owner, revision: 2)
        XCTAssertNil(session.preview(for: edited), "Live ownership is checked before observer delivery")
        XCTAssertNil(session.preview(for: .init(documentOwner: UUID(), revision: 1)))
        session.sourceDidChange(to: edited)
        XCTAssertNil(session.preview(for: identity))
        XCTAssertEqual(session.baseline, "baseline")
        session.resetForDocument()
        XCTAssertNil(session.baseline)
    }

    @MainActor
    func testStaleComparisonRequestCannotClearTheCurrentDocumentsBaseline() {
        let stale = MarkdownComparisonSession.SourceIdentity(documentOwner: UUID(), revision: 1)
        let current = MarkdownComparisonSession.SourceIdentity(documentOwner: UUID(), revision: 1)
        let session = MarkdownComparisonSession()
        session.snapshot("current document baseline", owner: current.documentOwner)
        XCTAssertNil(session.compare(source: "obsolete source", identity: stale, currentIdentity: { current }) { _, _ in
            XCTFail("A superseded request must not reach the renderer")
            return self.preview("obsolete")
        })
        XCTAssertEqual(session.baseline, "current document baseline")
        XCTAssertFalse(session.isComparing)
        XCTAssertNil(session.errorMessage)
    }

    @MainActor
    func testDocumentChangeWhileComparingCannotLeakThePriorBaselineOrError() async throws {
        let gate = ComparisonGate<SemanticDiffPreview>()
        var identity = MarkdownComparisonSession.SourceIdentity(documentOwner: UUID(), revision: 1)
        let session = MarkdownComparisonSession()
        session.snapshot("private earlier document", owner: identity.documentOwner)
        let work = try XCTUnwrap(session.compare(source: "current", identity: identity, currentIdentity: { identity }) { _, _ in
            try await gate.suspend()
        })
        await gate.waitUntilStarted()
        identity = .init(documentOwner: UUID(), revision: 1)
        gate.fail()
        await work.value
        XCTAssertNil(session.baseline)
        XCTAssertNil(session.preview(for: identity))
        XCTAssertNil(session.errorMessage)
    }

    private var inputURL: URL { URL(fileURLWithPath: "/selected.md") }

    private func file(_ bytes: Data) throws -> URL {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("baseline.md")
        try bytes.write(to: url)
        return url
    }

    private func preview(_ html: String) -> SemanticDiffPreview {
        SemanticDiffPreview(html: html, metrics: SemanticDiffReport(oldName: "Before", newName: "Current",
            stats: SemanticDiffStats(unchangedBlocks: 0, insertedBlocks: 1, deletedBlocks: 0,
                modifiedBlocks: 0, wordsInserted: 1, wordsDeleted: 0, similarityRatio: 0)))
    }
}

private struct ComparisonFailure: LocalizedError {
    var errorDescription: String? { "The selected provider failed." }
}

/// Intentionally ignores cancellation, like an already-running file-provider
/// read or synchronous WASM call. Ownership must reject its late completion.
@MainActor
private final class ComparisonGate<Value> {
    private var continuation: CheckedContinuation<Value, Error>?
    private var onStart: CheckedContinuation<Void, Never>?
    private var started = false

    func suspend() async throws -> Value {
        try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
            started = true
            onStart?.resume()
            onStart = nil
        }
    }

    func waitUntilStarted() async {
        if started { return }
        await withCheckedContinuation { onStart = $0 }
    }

    func succeed(_ value: Value) {
        continuation?.resume(returning: value)
        continuation = nil
    }

    func fail() {
        continuation?.resume(throwing: ComparisonFailure())
        continuation = nil
    }
}
