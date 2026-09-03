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
            fitToPages: 0,
            customizePDFTypography: nil,
            pdfBaseFontSize: nil,
            pdfHeadingScale: nil,
            pdfTableFontSize: nil,
            customCSS: nil
        )

        XCTAssertThrowsError(try store.save(draft))
    }

    func testSchemaOneDraftWithoutPDFTypographyStillLoads() throws {
        let store = makeStore()
        try FileManager.default.createDirectory(
            at: store.fileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
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

        let restored = try XCTUnwrap(store.load())
        XCTAssertEqual(restored.source, "# Existing draft")
        XCTAssertNil(restored.customizePDFTypography)
        XCTAssertNil(restored.pdfBaseFontSize)
    }

    func testOversizedCustomStylesheetIsRefused() {
        let store = makeStore()
        let original = makeDraft(source: "# Styled")
        let draft = MarkdownActiveDraft(
            schema: original.schema,
            savedAtMilliseconds: original.savedAtMilliseconds,
            source: original.source,
            title: original.title,
            author: original.author,
            fontFamily: original.fontFamily,
            rendererDarkMode: original.rendererDarkMode,
            tableOfContents: original.tableOfContents,
            tableOfContentsDepth: original.tableOfContentsDepth,
            pageNumbers: original.pageNumbers,
            codeLineNumbers: original.codeLineNumbers,
            language: original.language,
            microtypeProtrusion: original.microtypeProtrusion,
            fitToPages: original.fitToPages,
            customizePDFTypography: original.customizePDFTypography,
            pdfBaseFontSize: original.pdfBaseFontSize,
            pdfHeadingScale: original.pdfHeadingScale,
            pdfTableFontSize: original.pdfTableFontSize,
            customCSS: String(
                repeating: "x",
                count: MarkdownActiveDraft.maximumCustomCSSBytes + 1
            )
        )

        XCTAssertThrowsError(try store.save(draft)) { error in
            XCTAssertEqual(error as? MarkdownDraftStore.StoreError, .invalidDraft)
        }
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
            fitToPages: 12,
            customizePDFTypography: true,
            pdfBaseFontSize: 12,
            pdfHeadingScale: 1.25,
            pdfTableFontSize: 9.5,
            customCSS: "body { color: #123456; }"
        )
    }
}
