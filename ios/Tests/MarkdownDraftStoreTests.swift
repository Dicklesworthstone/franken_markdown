import XCTest
@testable import FrankenMarkdown

final class MarkdownDraftStoreTests: XCTestCase {
    func testRoundTripPreservesDocumentAndSafeSettings() throws {
        let store = makeStore()
        let draft = makeDraft(source: "# Recovered\n\nPrivate words.")

        try store.save(draft)

        XCTAssertEqual(store.load(), draft)
    }

    func testMalformedDraftFailsClosed() throws {
        let store = makeStore()
        try FileManager.default.createDirectory(
            at: store.fileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("not-json".utf8).write(to: store.fileURL, options: .atomic)

        XCTAssertNil(store.load())
    }

    func testOversizedSourceIsRefused() {
        let store = makeStore()
        let source = String(repeating: "a", count: MarkdownDraftStore.maximumSourceBytes + 1)

        XCTAssertThrowsError(try store.save(makeDraft(source: source))) { error in
            XCTAssertEqual(error as? MarkdownDraftStore.StoreError, .invalidDraft)
        }
    }

    func testUnsafeRestoredRendererSettingIsRefused() throws {
        let store = makeStore()
        let draft = MarkdownActiveDraft(
            schema: MarkdownActiveDraft.currentSchema,
            savedAtMilliseconds: 1,
            source: "# Safe",
            title: "",
            author: "",
            fontFamily: "remote-font",
            rendererDarkMode: "auto",
            tableOfContents: false,
            tableOfContentsDepth: 3,
            pageNumbers: false,
            codeLineNumbers: false,
            language: "en",
            microtypeProtrusion: false,
            fitToPages: 0
        )

        XCTAssertThrowsError(try store.save(draft))
    }

    private func makeStore() -> MarkdownDraftStore {
        MarkdownDraftStore(fileURL: FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
            .appendingPathComponent("active-draft.json"))
    }

    private func makeDraft(source: String) -> MarkdownActiveDraft {
        MarkdownActiveDraft(
            schema: MarkdownActiveDraft.currentSchema,
            savedAtMilliseconds: 1_725_350_400_000,
            source: source,
            title: "Recovered",
            author: "Author",
            fontFamily: "serif",
            rendererDarkMode: "auto",
            tableOfContents: true,
            tableOfContentsDepth: 4,
            pageNumbers: true,
            codeLineNumbers: true,
            language: "en",
            microtypeProtrusion: true,
            fitToPages: 12
        )
    }
}
