import Foundation
import SwiftUI
import UIKit
import XCTest
@testable import FrankenMarkdown

final class MarkdownEditorLineIndexTests: XCTestCase {
    func testUTF16LinesMatchFoundationAcrossUnicodeAndLineEndings() throws {
        let source = "\u{feff}# 🧪 cafe\u{301}\r\nquote\rnext\nother\u{85}after\u{2028}last\u{2029}"
        let index = try XCTUnwrap(MarkdownEditorLineIndex(source: source))
        let text = source as NSString
        XCTAssertEqual(index.utf16Count, text.length)
        for offset in 0..<text.length {
            let expected = text.lineRange(for: NSRange(location: offset, length: 0))
            let line = index.lines[index.lineIndex(containing: offset)]
            XCTAssertEqual(line.start, expected.location, "UTF-16 offset \(offset)")
            XCTAssertEqual(line.end, NSMaxRange(expected), "UTF-16 offset \(offset)")
        }
        XCTAssertEqual(index.lines.last?.start, text.length)
        XCTAssertEqual(index.lineNumber(startingAt: text.length), index.lines.count)
        XCTAssertNil(index.lineNumber(startingAt: 2))
    }

    func test750KBAndLargerDocumentsKeepFullTailAndBoundEveryViewport() throws {
        for minimumBytes in [750_000, 1_500_000] {
            let source = Self.largeSource(minimumBytes: minimumBytes)
            let index = try XCTUnwrap(MarkdownEditorLineIndex(source: source))
            let text = source as NSString
            XCTAssertGreaterThanOrEqual(source.utf8.count, minimumBytes)
            XCTAssertEqual(index.utf16Count, text.length)
            XCTAssertEqual(index.lines.last?.end, text.length)
            XCTAssertTrue(source.hasSuffix("LAST-LINE-🧪-PRESERVED"))
            for offset in stride(from: 0, to: text.length, by: max(1, text.length / 97)) {
                let line = index.lines[index.lineIndex(containing: offset)]
                let expected = text.lineRange(for: NSRange(location: offset, length: 0))
                XCTAssertEqual(line.start, expected.location)
                let window = index.highlightWindow(around: NSRange(location: offset, length: 600))
                XCTAssertLessThanOrEqual(window.length, MarkdownEditorLineIndex.maximumHighlightLength)
                XCTAssertLessThanOrEqual(window.location, offset)
                XCTAssertGreaterThan(NSMaxRange(window), offset)
                XCTAssertLessThanOrEqual(NSMaxRange(window), text.length)
                let first = index.lineIndex(containing: window.location)
                let last = index.lineIndex(containing: max(window.location, NSMaxRange(window) - 1))
                XCTAssertLessThanOrEqual(last - first + 1, MarkdownEditorLineIndex.maximumHighlightLines)
            }
        }
    }

    func testSingleHugeLineCannotExpandHighlightWorkToTheWholeSource() throws {
        let source = String(repeating: "🧪x", count: 500_000)
        let index = try XCTUnwrap(MarkdownEditorLineIndex(source: source))
        XCTAssertEqual(index.lines.count, 1)
        for offset in [0, index.utf16Count / 2, index.utf16Count - 50] {
            let window = index.highlightWindow(around: NSRange(location: offset, length: 100))
            XCTAssertLessThanOrEqual(window.length, MarkdownEditorLineIndex.maximumHighlightLength)
            XCTAssertLessThanOrEqual(window.location, offset)
            XCTAssertGreaterThan(NSMaxRange(window), offset)
        }
        XCTAssertEqual(index.utf16Count, source.utf16.count)
    }

    func testViewportRetainsFenceStateWithoutRescanningEarlierLines() throws {
        let opening = "# Heading\r\n" + String(repeating: "\u{60}", count: 3) + "swift\r\n"
        let closing = String(repeating: "\u{60}", count: 3) + "\r\nTail"
        let source = opening + String(repeating: "let value = 42\r\n", count: 20_000) + closing
        let index = try XCTUnwrap(MarkdownEditorLineIndex(source: source))
        let middle = index.lines[index.lines.count / 2]
        XCTAssertEqual(middle.fenceCharacter, 96)
        XCTAssertNil(index.lines.last?.fenceCharacter)
        let window = index.highlightWindow(around: NSRange(location: middle.start, length: 120))
        XCTAssertGreaterThan(window.location, opening.utf16.count)
        XCTAssertEqual(index.lines[index.lineIndex(containing: window.location)].fenceCharacter, 96)
        XCTAssertLessThanOrEqual(window.length, MarkdownEditorLineIndex.maximumHighlightLength)
    }

    func testEmptyTrailingAndCancelledIndexes() throws {
        let empty = try XCTUnwrap(MarkdownEditorLineIndex(source: ""))
        XCTAssertEqual(empty.lines.count, 1)
        XCTAssertEqual(empty.lineNumber(startingAt: 0), 1)
        XCTAssertEqual(empty.highlightWindow(around: NSRange(location: 0, length: 100)).length, 0)
        let trailing = try XCTUnwrap(MarkdownEditorLineIndex(source: "one\r\ntwo\n"))
        XCTAssertEqual(trailing.lines.map(\.start), [0, 5, 9])
        XCTAssertEqual(trailing.lineNumber(startingAt: 9), 3)
        var checks = 0
        let cancelled = MarkdownEditorLineIndex(
            source: Self.largeSource(minimumBytes: 750_000),
            isCancelled: {
                checks += 1
                return checks >= 3
            }
        )
        XCTAssertNil(cancelled)
        XCTAssertEqual(checks, 3)
    }

    @MainActor
    func testLargeEditorDecorationPreservesAllCharactersAndSelection() async throws {
        var source = Self.largeSource(minimumBytes: 750_000)
        let original = source
        let editor = MarkdownCodeEditor(
            text: Binding(get: { source }, set: { source = $0 }),
            isFocused: .constant(false)
        )
        let coordinator = MarkdownCodeEditor.Coordinator(editor)
        let container = MarkdownEditorContainer(frame: CGRect(x: 0, y: 0, width: 390, height: 700))
        let view = container.textView
        view.delegate = coordinator
        container.layoutIfNeeded()
        coordinator.updatePresentation(in: view, replacingText: original)
        defer { coordinator.cancelPendingWork() }

        let deadline = ContinuousClock.now.advanced(by: .seconds(10))
        while view.lineIndex == nil, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertNotNil(view.lineIndex, "The complete background index should install for the current source")
        let selection = NSRange(location: 2, length: 7)
        view.selectedRange = selection
        let observer = CharacterEditObserver()
        view.textStorage.delegate = observer
        coordinator.scheduleHighlight(in: view)
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(observer.characterEdits, 0, "Decoration must not replace source characters or reset edit ownership")
        XCTAssertEqual(view.textStorage.string, original)
        XCTAssertEqual(source, original)
        XCTAssertEqual(view.selectedRange, selection)
        XCTAssertTrue(view.layoutManager.allowsNonContiguousLayout)
    }

    private static func largeSource(minimumBytes: Int) -> String {
        let section = "# Heading 🧪\r\nSome **bold** and _italic_ text, [links](https://example.com), and cafe\u{301}.\r\n\r\n"
        return String(repeating: section, count: minimumBytes / section.utf8.count + 1)
            + "LAST-LINE-🧪-PRESERVED"
    }
}

private final class CharacterEditObserver: NSObject, NSTextStorageDelegate {
    var characterEdits = 0

    func textStorage(
        _ textStorage: NSTextStorage,
        didProcessEditing editedMask: NSTextStorage.EditActions,
        range editedRange: NSRange,
        changeInLength delta: Int
    ) {
        if editedMask.contains(.editedCharacters) { characterEdits += 1 }
    }
}
