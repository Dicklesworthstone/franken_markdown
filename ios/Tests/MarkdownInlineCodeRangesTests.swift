import XCTest
@testable import FrankenMarkdown

final class MarkdownInlineCodeRangesTests: XCTestCase {
    private func decorated(_ source: String, maximumLength: Int = 2_200) -> [String] {
        let text = source as NSString
        return MarkdownInlineCodeRanges.find(
            in: text,
            range: NSRange(location: 0, length: text.length),
            maximumLength: maximumLength
        ).map { text.substring(with: $0) }
    }

    func testSingleAndVariableLengthDelimiters() {
        XCTAssertEqual(decorated("one `code` and ``a `literal` tick`` and ```three```"), [
            "`code`", "``a `literal` tick``", "```three```"
        ])
        XCTAssertEqual(decorated("`` `nested` ``"), ["`` `nested` ``"])
    }

    func testEarliestOpenerOwnsEnclosedAndCrossingRuns() {
        XCTAssertEqual(decorated("`outer ``inner`` outer`"), ["`outer ``inner`` outer`"])
        XCTAssertEqual(decorated("`first ``crossing` trailing``"), ["`first ``crossing`"])
        XCTAssertEqual(decorated("``unclosed outer `valid inner`"), ["`valid inner`"])
    }

    func testUnequalOrUnclosedRunsStayPlain() {
        XCTAssertEqual(decorated("unclosed `code"), [])
        XCTAssertEqual(decorated("```unequal``"), [])
        XCTAssertEqual(decorated("``unequal```"), [])
        XCTAssertEqual(decorated("before ``` after"), [])
    }

    func testLongUnmatchedRunsDoNotEnterBacktrackingRegex() {
        let run = String(repeating: "`", count: 19_000)
        XCTAssertEqual(decorated("a" + run + "x"), [])
        XCTAssertEqual(decorated("a" + run + "x `valid tail`"), ["`valid tail`"])
        let matching = String(repeating: "`", count: 9_000)
        XCTAssertEqual(decorated(matching + "x" + matching), [])
    }

    func testUnicodeCoordinatesUseUTF16WithoutSplittingCharacters() {
        let source = "🙂 prefix `café 👩🏽‍💻 漢字` suffix" as NSString
        let expected = source.range(of: "`café 👩🏽‍💻 漢字`")
        let result = MarkdownInlineCodeRanges.find(
            in: source, range: NSRange(location: 0, length: source.length)
        )
        XCTAssertEqual(result, [expected])
        XCTAssertEqual(source.substring(with: result[0]), "`café 👩🏽‍💻 漢字`")
    }

    func testDecorationLimitDoesNotHideLaterCode() {
        let long = "`" + String(repeating: "x", count: 2_200) + "`"
        XCTAssertEqual(decorated(long + " then `tail`"), ["`tail`"])
        XCTAssertEqual(decorated("`123` `1234`", maximumLength: 5), ["`123`"])
        XCTAssertEqual(decorated("`code`", maximumLength: 0), [])
    }

    func testLineAndExplicitRangeBoundariesAreRespected() {
        XCTAssertEqual(decorated("`first\nsecond`\r\n`valid`\u{2028}`also valid`"), [
            "`valid`", "`also valid`"
        ])
        let source = "outside `first`\nselected `second`\n" as NSString
        let selected = source.range(of: "selected `second`")
        let ranges = MarkdownInlineCodeRanges.find(in: source, range: selected)
        XCTAssertEqual(ranges, [source.range(of: "`second`")])
        XCTAssertEqual(MarkdownInlineCodeRanges.find(
            in: source, range: NSRange(location: NSNotFound, length: 1)
        ), [])
        XCTAssertEqual(MarkdownInlineCodeRanges.find(
            in: source, range: NSRange(location: source.length, length: Int.max)
        ), [])
    }
}
