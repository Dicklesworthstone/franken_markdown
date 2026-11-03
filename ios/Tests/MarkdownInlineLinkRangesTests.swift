import XCTest
@testable import FrankenMarkdown

final class MarkdownInlineLinkRangesTests: XCTestCase {
    private func matches(_ source: String) -> [MarkdownInlineLinkRanges.Match] {
        let text = source as NSString
        return MarkdownInlineLinkRanges.find(in: text, range: NSRange(location: 0, length: text.length))
    }

    private func decorated(_ source: String) -> [String] {
        let text = source as NSString
        return matches(source).map { text.substring(with: $0.range) }
    }

    private func automatic(_ source: String) -> [String] {
        let text = source as NSString
        return MarkdownInlineLinkRanges.findAutolinks(
            in: text, range: NSRange(location: 0, length: text.length)
        ).map { text.substring(with: $0) }
    }

    func testOrdinaryLinksImagesAndFiveStylingRanges() throws {
        let source = "[label](https://example.com) and ![image](asset.png)" as NSString
        let found = matches(source as String)
        XCTAssertEqual(found.count, 2)
        let link = try XCTUnwrap(found.first)
        XCTAssertEqual([
            link.opening, link.label, link.separator, link.destination, link.closing
        ].map { source.substring(with: $0) }, ["[", "label", "](", "https://example.com", ")"])
        let image = try XCTUnwrap(found.last)
        XCTAssertEqual([
            image.opening, image.label, image.separator, image.destination, image.closing
        ].map { source.substring(with: $0) }, ["![", "image", "](", "asset.png", ")"])
    }

    func testExistingLiteralLexicalGrammarIsRetained() {
        XCTAssertEqual(decorated("[]() [outer [inner](target) [nested](a(b)c)"), [
            "[]()", "[outer [inner](target)", "[nested](a(b)"
        ])
        XCTAssertEqual(decorated("[bad] text [good](target)"), ["[good](target)"])
        XCTAssertEqual(decorated("!![image](asset)"), ["![image](asset)"])
    }

    func testLargeUnmatchedBracketsAndDestinationsAreConsumedOnce() {
        let brackets = String(repeating: "[", count: 19_000)
        XCTAssertEqual(decorated(brackets + "unclosed"), [])
        XCTAssertEqual(decorated(brackets + "] broken [valid](tail)"), ["[valid](tail)"])
        XCTAssertEqual(decorated("[label](" + brackets + "no closing parenthesis"), [])
    }

    func testUnicodeRangesPreserveWideCharacters() throws {
        let source = "🙂 [café 👩🏽‍💻](https://example.com/漢字) tail" as NSString
        let match = try XCTUnwrap(matches(source as String).first)
        XCTAssertEqual(match.range, source.range(of: "[café 👩🏽‍💻](https://example.com/漢字)"))
        XCTAssertEqual(source.substring(with: match.label), "café 👩🏽‍💻")
        XCTAssertEqual(source.substring(with: match.destination), "https://example.com/漢字")
    }

    func testProtectedInlineCodeDoesNotBecomeALinkDecoration() {
        let source = "`[hidden](inside)` and [shown](outside)" as NSString
        let full = NSRange(location: 0, length: source.length)
        var protection = MarkdownInlineProtection(range: full)
        for code in MarkdownInlineCodeRanges.find(in: source, range: full) {
            protection.insert(code)
        }
        let visibleLinks = MarkdownInlineLinkRanges.find(in: source, range: full).filter { match in
            !protection.intersects(match.range)
        }
        XCTAssertEqual(visibleLinks.map { source.substring(with: $0.range) }, ["[shown](outside)"])
    }

    func testNewlinesAndExplicitWindowsAreNotCrossed() {
        XCTAssertEqual(decorated("[broken](destination\r\n[valid](tail)\u{2028}[also](valid)"), [
            "[valid](tail)", "[also](valid)"
        ])
        XCTAssertEqual(decorated("[broken\nlabel](target)"), [])
        let source = "outside [first](one)\nselected [second](two)\n" as NSString
        let selected = source.range(of: "selected [second](two)")
        XCTAssertEqual(MarkdownInlineLinkRanges.find(in: source, range: selected).map(\.range), [
            source.range(of: "[second](two)")
        ])
        XCTAssertEqual(MarkdownInlineLinkRanges.find(
            in: source, range: NSRange(location: NSNotFound, length: 1)
        ), [])
        XCTAssertEqual(MarkdownInlineLinkRanges.find(
            in: source, range: NSRange(location: source.length, length: Int.max)
        ), [])
    }

    func testAutomaticLinksKeepLowercaseSchemesAndNonemptyDestinations() {
        XCTAssertEqual(automatic("<http://a> <https://b?q=1> <http://> <https://> <HTTP://upper> <HtTpS://mixed>"), [
            "<http://a>", "<https://b?q=1>"
        ])
        XCTAssertEqual(automatic("<<http://a>"), ["<http://a>"])
    }

    func testAutomaticLinksPreserveCandidateOwnershipWithoutRescanning() {
        XCTAssertEqual(automatic("<https://outer<http://inner> <https://bad space <https://tail>"), [
            "<https://outer<http://inner>", "<https://tail>"
        ])
        let malformed = String(repeating: "<https://a", count: 1_900)
        XCTAssertEqual(malformed.utf16.count, 19_000)
        XCTAssertEqual(automatic(malformed), [])
        XCTAssertEqual(automatic(malformed + "\n<https://tail>"), ["<https://tail>"])
    }

    func testAutomaticLinksRespectICUUnicodeWhitespaceAndUTF16Ranges() throws {
        let separators: [UInt32] = [9, 10, 11, 12, 13, 32, 0x85, 0xA0, 0x1680,
                                    0x2000, 0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000]
        for value in separators {
            let scalar = try XCTUnwrap(UnicodeScalar(value))
            XCTAssertEqual(automatic("<https://bad\(scalar)suffix> <https://tail>"), ["<https://tail>"])
        }
        let source = "🙂 <https://example.com/漢字> <https://zero\u{200B}width>" as NSString
        let ranges = MarkdownInlineLinkRanges.findAutolinks(
            in: source, range: NSRange(location: 0, length: source.length)
        )
        XCTAssertEqual(ranges, [
            source.range(of: "<https://example.com/漢字>"),
            source.range(of: "<https://zero\u{200B}width>")
        ])
        let truncated = source.range(of: "<https://example.com/漢字")
        XCTAssertEqual(MarkdownInlineLinkRanges.findAutolinks(in: source, range: truncated), [])
    }

    func testProtectedCodeAndExplicitLinksKeepPriorityOverAutomaticLinks() {
        let source = "`<https://code>` [label](<https://destination>) <https://shown>" as NSString
        let full = NSRange(location: 0, length: source.length)
        var protection = MarkdownInlineProtection(range: full)
        for code in MarkdownInlineCodeRanges.find(in: source, range: full) {
            protection.insert(code)
        }
        for link in MarkdownInlineLinkRanges.find(in: source, range: full)
            where !protection.intersects(link.range) {
            protection.insert(link.range)
        }
        let visible = MarkdownInlineLinkRanges.findAutolinks(in: source, range: full).filter { candidate in
            !protection.intersects(candidate)
        }
        XCTAssertEqual(visible.map { source.substring(with: $0) }, ["<https://shown>"])
    }

    func testProtectionBitmapUsesAbsoluteUTF16OffsetsAndAdjacency() {
        var protection = MarkdownInlineProtection(range: NSRange(location: 100, length: 20))
        protection.insert(NSRange(location: 104, length: 3))
        XCTAssertTrue(protection.intersects(NSRange(location: 105, length: 1)))
        XCTAssertTrue(protection.intersects(NSRange(location: 101, length: 4)))
        XCTAssertFalse(protection.intersects(NSRange(location: 103, length: 1)))
        XCTAssertFalse(protection.intersects(NSRange(location: 107, length: 1)))
        XCTAssertFalse(protection.intersects(NSRange(location: 104, length: 0)))
        XCTAssertFalse(protection.intersects(NSRange(location: 0, length: 100)))
    }

    func testProtectionBitmapClipsCandidatesWithoutOverflow() {
        var protection = MarkdownInlineProtection(range: NSRange(location: 100, length: 20))
        protection.insert(NSRange(location: 95, length: 7))
        XCTAssertTrue(protection.intersects(NSRange(location: 100, length: 2)))
        XCTAssertFalse(protection.intersects(NSRange(location: 102, length: 1)))
        protection.insert(NSRange(location: 119, length: Int.max))
        XCTAssertTrue(protection.intersects(NSRange(location: 119, length: 1)))
        XCTAssertTrue(protection.intersects(NSRange(location: 0, length: Int.max)))
        XCTAssertFalse(protection.intersects(NSRange(location: NSNotFound, length: 1)))
        XCTAssertFalse(protection.intersects(NSRange(location: 120, length: Int.max)))
        let empty = MarkdownInlineProtection(range: NSRange(location: 0, length: 0))
        XCTAssertFalse(empty.intersects(NSRange(location: 0, length: Int.max)))
    }

    func testProtectionBitmapHandlesThousandsOfCompactLinks() {
        let source = String(repeating: "[]()", count: 4_750) as NSString
        let full = NSRange(location: 0, length: source.length)
        let links = MarkdownInlineLinkRanges.find(in: source, range: full)
        XCTAssertEqual(links.count, 4_750)
        var protection = MarkdownInlineProtection(range: full)
        for link in links {
            XCTAssertFalse(protection.intersects(link.range))
            protection.insert(link.range)
        }
        for link in links {
            XCTAssertTrue(protection.intersects(link.range))
        }
        XCTAssertFalse(protection.intersects(NSRange(location: source.length, length: 1)))
    }
}
