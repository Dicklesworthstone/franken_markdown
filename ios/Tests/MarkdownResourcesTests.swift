import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownResourcesTests: XCTestCase {
    func testByteDistinctUnicodeDestinationsKeepDistinctIdentityAndEquality() throws {
        let composed = attachment(key: "caf\u{00E9}.png")
        let decomposed = attachment(key: "cafe\u{0301}.png")
        XCTAssertEqual(composed.key, decomposed.key, "Swift canonical equality is not the resource identity contract")
        XCTAssertNotEqual(Data(composed.key.utf8), Data(decomposed.key.utf8))
        XCTAssertNotEqual(composed.id, decomposed.id)
        XCTAssertNotEqual(composed, decomposed)
        let first = try MarkdownResourceCollection(validating: [composed])
        let second = try MarkdownResourceCollection(validating: [decomposed])
        XCTAssertNotEqual(first, second, "Resource revision guards must observe byte-distinct destination changes")
        let both = try first.replacing(decomposed)
        XCTAssertEqual(both.entries.count, 2)
        let updated = attachment(key: decomposed.key, bytes: Data([99]))
        let replaced = try both.replacing(updated)
        XCTAssertEqual(replaced.entries[0], composed)
        XCTAssertEqual(replaced.entries[1], updated)
        XCTAssertEqual(replaced.removing(id: composed.id).entries, [updated])
    }

    func testKeyAdmissionUsesUTF8LimitsAndExplicitFontSlots() throws {
        let boundary = String(repeating: "é", count: MarkdownResourceCollection.maximumKeyBytes / 2)
        XCTAssertNoThrow(try MarkdownResourceCollection.validateKey(boundary, kind: .image))
        for key in ["", " \n", "nul\0key", "line\nkey", "return\rkey", boundary + "a"] {
            XCTAssertThrowsError(try MarkdownResourceCollection.validateKey(key, kind: .image)) { error in
                XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, .invalidKey)
            }
        }
        for slot in MarkdownFontSlot.allCases {
            XCTAssertNoThrow(try MarkdownResourceCollection.validateKey(slot.rawValue, kind: .font))
        }
        XCTAssertThrowsError(try MarkdownResourceCollection.validateKey("unknown-font", kind: .font))
        let image = attachment(key: "body-regular")
        let font = attachment(kind: .font, key: "body-regular")
        XCTAssertEqual(try MarkdownResourceCollection(validating: [image, font]).entries.count, 2)
        XCTAssertThrowsError(try MarkdownResourceCollection(validating: [image, image])) { error in
            XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, .duplicateKey)
        }
    }

    func testCollectionEnforcesIndividualAggregateAndImageCountBounds() throws {
        let full = Data(repeating: 1, count: MarkdownResourceCollection.maximumAssetBytes)
        let first = attachment(key: "first.png", bytes: full)
        let second = attachment(key: "second.png", bytes: full)
        let exact = try MarkdownResourceCollection(validating: [first, second])
        XCTAssertEqual(exact.byteCount, MarkdownResourceCollection.maximumTotalBytes)
        XCTAssertThrowsError(try exact.replacing(attachment(key: "extra.png"))) { error in
            XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, .oversizedCollection)
        }
        XCTAssertThrowsError(try MarkdownResourceCollection(validating: [
            attachment(bytes: Data(repeating: 1, count: MarkdownResourceCollection.maximumAssetBytes + 1))
        ])) { error in
            XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, .oversizedAsset)
        }
        let images = (0..<MarkdownResourceCollection.maximumImages).map { attachment(key: "\($0).png") }
        XCTAssertEqual(try MarkdownResourceCollection(validating: images).entries.count, 128)
        XCTAssertThrowsError(try MarkdownResourceCollection(validating: images + [attachment(key: "extra.png")])) { error in
            XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, .oversizedCollection)
        }
        for invalid in [attachment(bytes: Data()), attachment(filename: ""),
                        attachment(filename: String(repeating: "a", count: 1_025))] {
            XCTAssertThrowsError(try MarkdownResourceCollection(validating: [invalid])) { error in
                XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, .invalidFile)
            }
        }
    }

    func testResourceSnapshotsRoundTripExactKeysBytesAndEmptyCollections() async throws {
        let (store, _) = try makeStore()
        let resources = try MarkdownResourceCollection(validating: [
            attachment(key: "caf\u{00E9}.png", bytes: Data([0, 255, 1])),
            attachment(key: "cafe\u{0301}.png", bytes: Data([2, 0, 254])),
            attachment(kind: .font, key: "mono-regular", filename: "code.ttf", bytes: Data([0, 1, 0, 0, 99]))
        ])
        let identity = try await store.save(resources)
        let loaded = try await store.load(identity)
        XCTAssertEqual(loaded, resources)
        XCTAssertEqual(loaded.entries.map { Data($0.key.utf8) }, resources.entries.map { Data($0.key.utf8) })
        let emptyID = try await store.save(.empty)
        let empty = try await store.load(emptyID)
        XCTAssertEqual(empty, .empty)
        let original = try await store.load(identity)
        XCTAssertEqual(original, resources, "Publishing another snapshot does not change the earlier generation")
    }

    func testCorruptedAndTruncatedSnapshotBytesAreRefusedWithoutChangingOtherGenerations() async throws {
        let (store, directory) = try makeStore()
        let resources = try MarkdownResourceCollection(validating: [attachment(bytes: Data([1, 2, 3, 4]))])
        let preserved = try await store.save(resources)
        for corruption in [Data([1, 2, 3, 9]), Data([1, 2])] {
            let identity = try await store.save(resources)
            let file = directory.appendingPathComponent(identity.uuidString).appendingPathComponent("0.bin")
            try corruption.write(to: file)
            await expectResourceFailure(.invalidSnapshot) { _ = try await store.load(identity) }
        }
        let loaded = try await store.load(preserved)
        XCTAssertEqual(loaded, resources)
    }

    func testManifestBudgetsAreAdmittedBeforeReadingAttachmentFiles() async throws {
        let (store, directory) = try makeStore()
        let identity = UUID()
        let target = directory.appendingPathComponent(identity.uuidString)
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
        let entries: [[String: Any]] = (0..<3).map { index in
            ["kind": "image", "key": "\(index).png", "filename": "image.png",
             "byteCount": MarkdownResourceCollection.maximumAssetBytes,
             "sha256": Data(repeating: 0, count: 32).base64EncodedString()]
        }
        try JSONSerialization.data(withJSONObject: ["schema": 1, "entries": entries])
            .write(to: target.appendingPathComponent("manifest.json"))
        // No .bin files exist: the aggregate refusal must happen first.
        await expectResourceFailure(.oversizedCollection) { _ = try await store.load(identity) }
        try JSONSerialization.data(withJSONObject: ["schema": 2, "entries": []])
            .write(to: target.appendingPathComponent("manifest.json"))
        await expectResourceFailure(.invalidSnapshot) { _ = try await store.load(identity) }
    }

    func testDiscardOnlyRemovesExplicitlyOwnedSupersededSnapshots() async throws {
        let (store, directory) = try makeStore()
        let old = try await store.save(.empty)
        let current = try await store.save(.empty)
        let other = try await store.save(.empty)
        await store.discard([old, current, UUID()], keeping: current)
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.appendingPathComponent(old.uuidString).path))
        let kept = try await store.load(current)
        let unrelated = try await store.load(other)
        XCTAssertEqual(kept, .empty)
        XCTAssertEqual(unrelated, .empty)
    }

    func testSelectionReadsAnImmutableSnapshotAndPreservesExactRequestedDestination() async throws {
        let (store, directory) = try makeStore()
        let file = directory.deletingLastPathComponent().appendingPathComponent("diagram.svg")
        let original = Data("<svg xmlns=\"http://www.w3.org/2000/svg\"><text>Café 🧪</text></svg>".utf8)
        try original.write(to: file)
        let inferred = try await store.selectedFile(at: file, kind: .image, key: "")
        XCTAssertEqual(inferred.key, "diagram.svg")
        XCTAssertEqual(inferred.bytes, original)
        let destination = "../images/cafe\u{0301}.svg?variant=1#diagram"
        let explicit = try await store.selectedFile(at: file, kind: .image, key: destination)
        XCTAssertEqual(Data(explicit.key.utf8), Data(destination.utf8))
        try Data("<svg/>".utf8).write(to: file)
        XCTAssertEqual(explicit.bytes, original, "Later provider changes cannot mutate an attached snapshot")
        let identity = try await store.save(MarkdownResourceCollection(validating: [explicit]))
        let restored = try await store.load(identity)
        XCTAssertEqual(restored.entries.first?.bytes, original)
    }

    func testSVGAdmissionAllowsAMultibyteScalarSplitAtItsSniffingBoundary() async throws {
        let (store, directory) = try makeStore()
        let file = directory.deletingLastPathComponent().appendingPathComponent("unicode.svg")
        let prefix = "<svg xmlns=\"http://www.w3.org/2000/svg\"><text>"
        let source = prefix + String(repeating: "a", count: 4_095 - prefix.utf8.count) + "🧪</text></svg>"
        let bytes = Data(source.utf8)
        XCTAssertNil(String(data: bytes.prefix(4_096), encoding: .utf8), "Fixture cuts the four-byte scalar")
        try bytes.write(to: file)
        let selected = try await store.selectedFile(at: file, kind: .image, key: "unicode.svg")
        XCTAssertEqual(selected.bytes, bytes)
    }

    func testSelectionRejectsUnsupportedEmptyOversizedAndNonregularFiles() async throws {
        let (store, directory) = try makeStore()
        let parent = directory.deletingLastPathComponent()
        let empty = parent.appendingPathComponent("empty.png")
        try Data().write(to: empty)
        await expectResourceFailure(.invalidFile) { _ = try await store.selectedFile(at: empty, kind: .image, key: "x") }
        let text = parent.appendingPathComponent("text.png")
        try Data("An extension does not grant image admission.".utf8).write(to: text)
        await expectResourceFailure(.unsupportedFormat) { _ = try await store.selectedFile(at: text, kind: .image, key: "x") }
        let large = parent.appendingPathComponent("large.png")
        try Data([137, 80, 78, 71, 13, 10, 26, 10]).write(to: large)
        let handle = try FileHandle(forWritingTo: large)
        try handle.truncate(atOffset: UInt64(MarkdownResourceCollection.maximumAssetBytes + 1))
        try handle.close()
        await expectResourceFailure(.oversizedAsset) { _ = try await store.selectedFile(at: large, kind: .image, key: "x") }
        await expectResourceFailure(.invalidFile) { _ = try await store.selectedFile(at: parent, kind: .image, key: "x") }
        let link = parent.appendingPathComponent("linked.png")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: text)
        await expectResourceFailure(.invalidFile) { _ = try await store.selectedFile(at: link, kind: .image, key: "x") }
        await expectResourceFailure(.invalidFile) {
            _ = try await store.selectedFile(at: URL(string: "https://example.test/image.png")!, kind: .image, key: "x")
        }
    }

    func testFontSelectionRequiresASupportedSignatureAndExactSlot() async throws {
        let (store, directory) = try makeStore()
        let file = directory.deletingLastPathComponent().appendingPathComponent("selected.ttf")
        // These are signature-admission fixtures. Font parsing is independently
        // performed by the renderer; this test does not assert a valid font.
        for signature in [Data([0, 1, 0, 0]), Data("OTTO".utf8), Data("true".utf8)] {
            try (signature + Data(repeating: 0, count: 12)).write(to: file)
            let selected = try await store.selectedFile(at: file, kind: .font, key: "body-regular")
            XCTAssertEqual(selected.bytes.prefix(4), signature)
        }
        await expectResourceFailure(.invalidKey) { _ = try await store.selectedFile(at: file, kind: .font, key: "") }
        try Data("wOFFunsupported".utf8).write(to: file)
        await expectResourceFailure(.unsupportedFormat) { _ = try await store.selectedFile(at: file, kind: .font, key: "mono-regular") }
    }

    func testCancelledStoreOperationsStopBeforeFilesystemAccess() async throws {
        let (store, directory) = try makeStore()
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            do { _ = try await store.save(.empty); XCTFail("Cancelled save must not create a snapshot") }
            catch { XCTAssertTrue(error is CancellationError) }
            do { _ = try await store.load(UUID()); XCTFail("Cancelled load must not access missing files") }
            catch { XCTAssertTrue(error is CancellationError) }
            do {
                _ = try await store.selectedFile(at: directory.appendingPathComponent("missing.svg"), kind: .image, key: "x")
                XCTFail("Cancelled selection must not begin coordination")
            } catch { XCTAssertTrue(error is CancellationError) }
        }
        await task.value
        XCTAssertFalse(FileManager.default.fileExists(atPath: directory.path))
    }

    func testResourceUploadPreservesBytesOrderAndChecksumsAcrossAssetAndChunkBoundaries() async throws {
        var entries: [MarkdownResourceAttachment] = []
        for (index, size) in [1, 65_535, 65_536, 65_537, 196_615].enumerated() {
            let storage = Data((0..<(size + 7)).map { UInt8($0 % 251) })
            entries.append(attachment(key: "\(index).png", bytes: storage[7..<(size + 7)]))
        }
        let resources = try MarkdownResourceCollection(validating: entries)
        let upload = MarkdownResourceUpload(resources: resources, resourceSetID: 7)
        let manifest = try await upload.manifest()
        XCTAssertEqual(manifest.start, .init(resourceSetID: 7, entryCount: entries.count,
                                           byteLength: resources.byteCount, chunkSize: 65_536))
        XCTAssertTrue(JSONSerialization.isValidJSONObject(manifest.arguments))
        XCTAssertEqual(manifest.entries.map(\.key), entries.map(\.key))
        XCTAssertEqual(manifest.entries.map(\.byteLength), entries.map { $0.bytes.count })
        var received = entries.map { _ in Data() }
        var count = 0, total = 0, expectedAsset = 0
        while let chunk = try await upload.nextChunk() {
            XCTAssertEqual(chunk.resourceSetID, 7)
            XCTAssertEqual(chunk.index, count)
            XCTAssertEqual(chunk.assetIndex, expectedAsset)
            let decoded = try XCTUnwrap(Data(base64Encoded: chunk.base64))
            XCTAssertEqual(decoded.count, min(65_536, entries[expectedAsset].bytes.count - received[expectedAsset].count))
            XCTAssertEqual(chunk.byteLength, decoded.count)
            XCTAssertLessThanOrEqual(chunk.base64.utf8.count, 87_384)
            XCTAssertEqual(chunk.checksum, referenceCRC32(decoded))
            XCTAssertTrue(JSONSerialization.isValidJSONObject(chunk.arguments))
            received[expectedAsset].append(decoded)
            total += decoded.count
            XCTAssertEqual(chunk.receivedBytes, total)
            if received[expectedAsset].count == entries[expectedAsset].bytes.count { expectedAsset += 1 }
            count += 1
        }
        XCTAssertEqual(received, entries.map(\.bytes))
        let receipt = try await upload.finish()
        XCTAssertEqual(receipt, .init(resourceSetID: 7, byteLength: resources.byteCount,
                                     chunkCount: count, entryCount: entries.count))
        XCTAssertTrue(JSONSerialization.isValidJSONObject(receipt.arguments))
        await expectTransferFailure(.closed) { _ = try await upload.manifest() }
        await expectTransferFailure(.closed) { _ = try await upload.nextChunk() }
        await expectTransferFailure(.closed) { _ = try await upload.finish() }
    }

    func testEmptyResourceUploadAndInvalidLifecycleCannotFabricateCompletion() async throws {
        let empty = MarkdownResourceUpload(resources: .empty, resourceSetID: 1)
        await expectTransferFailure(.notStarted) { _ = try await empty.nextChunk() }
        await expectTransferFailure(.notStarted) { _ = try await empty.finish() }
        _ = try await empty.manifest()
        let chunk = try await empty.nextChunk()
        XCTAssertNil(chunk)
        let receipt = try await empty.finish()
        XCTAssertEqual(receipt, .init(resourceSetID: 1, byteLength: 0, chunkCount: 0, entryCount: 0))
        for identifier in [0, -1, 9_007_199_254_740_992] {
            let invalid = MarkdownResourceUpload(resources: .empty, resourceSetID: identifier)
            await expectTransferFailure(.invalidManifest) { _ = try await invalid.manifest() }
        }
        let resources = try MarkdownResourceCollection(validating: [attachment()])
        let incomplete = MarkdownResourceUpload(resources: resources, resourceSetID: 2)
        _ = try await incomplete.manifest()
        await expectTransferFailure(.incomplete) { _ = try await incomplete.finish() }
        await incomplete.abort()
        await expectTransferFailure(.closed) { _ = try await incomplete.nextChunk() }
        await expectTransferFailure(.closed) { _ = try await incomplete.finish() }
    }

    func testCancelledUploadDoesNotAdvanceTheNextAssetChunk() async throws {
        let bytes = Data(repeating: 19, count: 65_537)
        let resources = try MarkdownResourceCollection(validating: [attachment(bytes: bytes)])
        let upload = MarkdownResourceUpload(resources: resources, resourceSetID: 11)
        _ = try await upload.manifest()
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            do { _ = try await upload.nextChunk(); XCTFail("Cancelled upload must not advance") }
            catch { XCTAssertTrue(error is CancellationError) }
        }
        await task.value
        let firstValue = try await upload.nextChunk()
        let first = try XCTUnwrap(firstValue)
        XCTAssertEqual(first.index, 0)
        XCTAssertEqual(first.assetIndex, 0)
        XCTAssertEqual(first.receivedBytes, 65_536)
        XCTAssertEqual(Data(base64Encoded: first.base64), bytes.prefix(65_536))
        let secondValue = try await upload.nextChunk()
        let second = try XCTUnwrap(secondValue)
        XCTAssertEqual(second.index, 1)
        XCTAssertEqual(second.receivedBytes, 65_537)
        _ = try await upload.finish()
    }

    @MainActor
    func testImportedResourcesAndSourceRecoverTogetherInANewRendererModel() async throws {
        let (draftStore, directory) = try makeModelStore()
        let model = MarkdownRendererModel(draftStore: draftStore)
        try await waitUntil("initial recovery") { model.draftRecoveryIsComplete }
        let source = "# Attached draft\n\n![Diagram](cafe\u{0301}.svg)\n\nExact Café 🧪 source."
        model.source = source
        model.documentTitle = "Attached draft"
        model.allowRawHtml = true
        let image = Data("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><rect width=\"8\" height=\"8\"/></svg>".utf8)
        let file = directory.appendingPathComponent("selected.svg")
        try image.write(to: file)
        try await model.importResource(from: file, kind: .image, key: "cafe\u{0301}.svg", owner: model.resourceDocumentOwner)
        try await waitUntil("attachment and source save") { model.draftStatus == "Saved locally" }
        let saved = await draftStore.load()
        XCTAssertNotNil(saved?.resourceSnapshotID)
        XCTAssertEqual(saved?.source, source)
        let restored = MarkdownRendererModel(draftStore: draftStore)
        try await waitUntil("resource recovery") { restored.draftRecoveryIsComplete }
        XCTAssertTrue(restored.draftWasRecovered)
        XCTAssertEqual(Data(restored.source.utf8), Data(source.utf8))
        XCTAssertEqual(restored.resources, model.resources)
        XCTAssertEqual(restored.resources.entries.first?.bytes, image)
        XCTAssertEqual(restored.resources.entries.first.map { Data($0.key.utf8) }, Data("cafe\u{0301}.svg".utf8))
        XCTAssertFalse(restored.hasUnavailableResources)
        XCTAssertFalse(restored.allowRawHtml, "Recovered attachments do not restore raw HTML trust")
    }

    @MainActor
    func testLatePickerOwnerCannotAttachToOrRemoveFromAReplacementDocument() async throws {
        let (draftStore, directory) = try makeModelStore()
        let model = MarkdownRendererModel(draftStore: draftStore)
        try await waitUntil("initial recovery") { model.draftRecoveryIsComplete }
        let file = directory.appendingPathComponent("selected.svg")
        try Data("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".utf8).write(to: file)
        let oldOwner = model.resourceDocumentOwner
        try await model.importResource(from: file, kind: .image, key: "old.svg", owner: oldOwner)
        let oldID = try XCTUnwrap(model.resources.entries.first?.id)
        model.resetResourcesForNewDocument()
        model.documentIdentity = UUID()
        model.source = "# Replacement document"
        XCTAssertNotEqual(model.resourceDocumentOwner, oldOwner)
        XCTAssertEqual(model.resources, .empty)
        do {
            try await model.importResource(from: file, kind: .image, key: "late.svg", owner: oldOwner)
            XCTFail("A picker from the replaced document must not grant its file to the new one")
        } catch { XCTAssertTrue(error is CancellationError) }
        try await model.importResource(from: file, kind: .image, key: "new.svg", owner: model.resourceDocumentOwner)
        do {
            try await model.removeResource(id: oldID, owner: oldOwner)
            XCTFail("A stale removal must not publish a resource snapshot for the new document")
        } catch { XCTAssertTrue(error is CancellationError) }
        XCTAssertEqual(model.resources.entries.map(\.key), ["new.svg"])
        XCTAssertFalse(model.resourcesAreLoading)
        try await waitUntil("replacement source save") { model.draftStatus == "Saved locally" }
        let restored = MarkdownRendererModel(draftStore: draftStore)
        try await waitUntil("replacement recovery") { restored.draftRecoveryIsComplete }
        XCTAssertEqual(restored.source, "# Replacement document")
        XCTAssertEqual(restored.resources.entries.map(\.key), ["new.svg"])
    }

    @MainActor
    func testInitialDocumentIdentityAdoptionKeepsTheResourceOwnerAndSnapshot() async throws {
        let (draftStore, directory) = try makeModelStore()
        let model = MarkdownRendererModel(draftStore: draftStore)
        try await waitUntil("initial recovery") { model.draftRecoveryIsComplete }
        model.source = "# Untitled with attachments\n\n![Diagram](diagram.svg)"
        let file = directory.appendingPathComponent("diagram.svg")
        try Data("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".utf8).write(to: file)
        let owner = model.resourceDocumentOwner
        try await model.importResource(from: file, kind: .image, key: "diagram.svg", owner: owner)
        try await waitUntil("untitled attachment save") { model.draftStatus == "Saved locally" }
        let before = await draftStore.load()
        let identity = UUID()
        model.documentIdentity = identity
        model.source += "\n\nEdits made while choosing the first save destination."
        model.scheduleDraftSave()
        try await waitUntil("associated document save") { model.draftStatus == "Saved locally" }
        let after = await draftStore.load()
        XCTAssertEqual(model.resourceDocumentOwner, owner)
        XCTAssertEqual(model.resources.entries.map(\.key), ["diagram.svg"])
        XCTAssertEqual(after?.documentIdentity, identity)
        XCTAssertNotNil(before?.resourceSnapshotID)
        XCTAssertEqual(after?.resourceSnapshotID, before?.resourceSnapshotID)
        XCTAssertEqual(after?.source, model.source)
    }

    @MainActor
    func testDamagedAttachmentRecoveryKeepsSourceAndBlocksExportUntilExplicitDiscard() async throws {
        let (draftStore, directory) = try makeModelStore()
        let model = MarkdownRendererModel(draftStore: draftStore)
        try await waitUntil("initial recovery") { model.draftRecoveryIsComplete }
        let source = "# Recoverable source\n\nATTACHMENT_RECOVERY_SOURCE_MARKER"
        model.source = source
        let file = directory.appendingPathComponent("diagram.svg")
        let bytes = Data("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".utf8)
        try bytes.write(to: file)
        try await model.importResource(from: file, kind: .image, key: "diagram.svg", owner: model.resourceDocumentOwner)
        try await waitUntil("attachment save") { model.draftStatus == "Saved locally" }
        let saved = await draftStore.load()
        let snapshotID = try XCTUnwrap(saved?.resourceSnapshotID)
        let persistedImage = directory.appendingPathComponent("attachments")
            .appendingPathComponent(snapshotID.uuidString).appendingPathComponent("0.bin")
        var damaged = bytes
        damaged[0] ^= 1
        try damaged.write(to: persistedImage)
        let restored = MarkdownRendererModel(draftStore: draftStore)
        try await waitUntil("damaged attachment recovery") { restored.draftRecoveryIsComplete }
        XCTAssertEqual(restored.source, source)
        XCTAssertTrue(restored.hasUnavailableResources)
        XCTAssertEqual(restored.resources, .empty)
        do {
            _ = try await restored.exportHtml()
            XCTFail("An unavailable attachment snapshot must not silently publish without its resources")
        } catch {
            XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, .invalidSnapshot)
        }
        try await restored.discardUnavailableResources(owner: restored.resourceDocumentOwner)
        try await waitUntil("explicit discard save") { restored.draftStatus == "Saved locally" }
        XCTAssertFalse(restored.hasUnavailableResources)
        XCTAssertEqual(restored.source, source)
        let discarded = await draftStore.load()
        XCTAssertNil(discarded?.resourceSnapshotID)
        let artifact = try await restored.exportHtml()
        addTeardownBlock { try? FileManager.default.removeItem(at: artifact.fileURL.deletingLastPathComponent()) }
        let html = try String(contentsOf: artifact.fileURL, encoding: .utf8)
        XCTAssertTrue(html.contains("ATTACHMENT_RECOVERY_SOURCE_MARKER"))
    }

    @MainActor
    private func waitUntil(_ label: String, condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now.advanced(by: .seconds(5))
        while !condition(), ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        guard condition() else {
            XCTFail("Timed out waiting for \(label)")
            throw ModelWaitError.timeout
        }
    }

    private enum ModelWaitError: Error { case timeout }

    private func makeModelStore() throws -> (MarkdownDraftStore, URL) {
        let (_, resourceDirectory) = try makeStore()
        let directory = resourceDirectory.deletingLastPathComponent()
        return (MarkdownDraftStore(fileURL: directory.appendingPathComponent("active-draft.json")), directory)
    }

    private func attachment(
        kind: MarkdownResourceKind = .image, key: String = "image.png",
        filename: String = "selected.png", bytes: Data = Data([1])
    ) -> MarkdownResourceAttachment {
        .init(kind: kind, key: key, filename: filename, bytes: bytes)
    }

    private func makeStore() throws -> (MarkdownResourceStore, URL) {
        let parent = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: parent) }
        let directory = parent.appendingPathComponent("resources", isDirectory: true)
        return (MarkdownResourceStore(directory: directory), directory)
    }

    private func expectResourceFailure(
        _ expected: MarkdownResourceCollection.ResourceError, operation: () async throws -> Void
    ) async {
        do { try await operation(); XCTFail("Expected attachment failure: \(expected)") }
        catch { XCTAssertEqual(error as? MarkdownResourceCollection.ResourceError, expected) }
    }

    private func expectTransferFailure(
        _ expected: MarkdownSourceUpload.TransferError, operation: () async throws -> Void
    ) async {
        do { try await operation(); XCTFail("Expected attachment transfer failure: \(expected)") }
        catch { XCTAssertEqual(error as? MarkdownSourceUpload.TransferError, expected) }
    }

    private func referenceCRC32(_ bytes: Data) -> String {
        var crc: UInt32 = 0xffff_ffff
        for byte in bytes {
            crc ^= UInt32(byte)
            for _ in 0..<8 { crc = (crc >> 1) ^ (crc & 1 == 0 ? 0 : 0xedb8_8320) }
        }
        return String(format: "%08x", crc ^ 0xffff_ffff)
    }
}
