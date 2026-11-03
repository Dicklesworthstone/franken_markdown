import XCTest
@testable import FrankenMarkdown

final class MarkdownSourceMetricsTests: XCTestCase {
    func testUnicodeCountsAndCRLFOutlineCoordinates() throws {
        let source = "# Café 👩🏽‍💻\r\n\none\ttwo\u{00A0}three\n"
        let metrics = try XCTUnwrap(MarkdownSourceMetrics.measure(source))
        XCTAssertEqual(metrics.byteCount, 41)
        XCTAssertEqual(metrics.characterCount, 24)
        XCTAssertEqual(metrics.wordCount, 6)
        XCTAssertEqual(metrics.headings, [
            .init(level: 1, title: "Café 👩🏽‍💻", lineNumber: 1)
        ])

        let lines = try XCTUnwrap(MarkdownSourceMetrics.measure(
            "first\r\n## Second\r\n\r\n### Fourth\n"
        ))
        XCTAssertEqual(lines.headings.map(\.lineNumber), [2, 4])
        XCTAssertEqual(lines.headings.map(\.id), [2, 4])
    }

    func testLargeSourceKeepsExactCountsAndFinalHeading() throws {
        let section = "## Section\n\nalpha beta gamma delta\n\n"
        let repetitions = 50_000
        let source = String(repeating: section, count: repetitions) + "# Final sentinel\n"
        XCTAssertGreaterThan(source.utf8.count, 1_500_000)
        let metrics = try XCTUnwrap(MarkdownSourceMetrics.measure(source))
        XCTAssertEqual(metrics.byteCount, section.utf8.count * repetitions + 17)
        XCTAssertEqual(metrics.characterCount, metrics.byteCount)
        XCTAssertEqual(metrics.wordCount, 6 * repetitions + 3)
        XCTAssertEqual(metrics.headings.count, repetitions + 1)
        XCTAssertEqual(metrics.headings.last?.title, "Final sentinel")
        XCTAssertEqual(metrics.headings.last?.lineNumber, 4 * repetitions + 1)
    }

    func testWordsMatchWhitespaceSemanticsWithoutAllocatingTokenArrays() throws {
        let source = "\t \nalpha\u{00A0}beta\r\ngamma\u{2028}delta\u{2029}\t"
        let metrics = try XCTUnwrap(MarkdownSourceMetrics.measure(source))
        XCTAssertEqual(metrics.wordCount, 4)
        XCTAssertEqual(MarkdownSourceMetrics.measure("")?.wordCount, 0)
        XCTAssertEqual(MarkdownSourceMetrics.measure("\r\n\t ")?.wordCount, 0)
    }

    func testCancelledScanDoesNotPublishPartialMetrics() {
        let source = String(repeating: "alpha beta\n", count: 100_000)
        var checks = 0
        let result = MarkdownSourceMetrics.measure(source) {
            checks += 1
            return checks > 2
        }
        XCTAssertNil(result)
        XCTAssertLessThanOrEqual(checks, 3)
    }

    func testInvalidHeadingMarkersDoNotCreateOutlineEntries() throws {
        let source = "#not a heading\n####### too deep\n  ## Valid  \n"
        let metrics = try XCTUnwrap(MarkdownSourceMetrics.measure(source))
        XCTAssertEqual(metrics.headings, [
            .init(level: 2, title: "Valid", lineNumber: 3)
        ])
    }
}
