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
}
