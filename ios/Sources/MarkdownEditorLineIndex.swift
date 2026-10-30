import Foundation

/// Source coordinates used by the native editor. Building the index is linear
/// and happens off the main actor; scrolling only performs binary lookups.
/// All offsets are UTF-16, matching UITextView and NSAttributedString.
struct MarkdownEditorLineIndex: Sendable {
    struct Line: Sendable {
        let start: Int
        let contentEnd: Int
        let end: Int
        let fenceCharacter: UInt16?
    }

    static let maximumHighlightLength = 32_768
    static let maximumHighlightLines = 160
    static let maximumLexicalLineLength = 20_000

    let utf16Count: Int
    let lines: [Line]

    init?(source: String, isCancelled: () -> Bool = { false }) {
        guard !isCancelled() else { return nil }
        let units = Array(source.utf16)
        var result: [Line] = []
        var start = 0
        var cursor = 0
        var fence: UInt16?

        func appendLine(contentEnd: Int, end: Int) {
            result.append(Line(start: start, contentEnd: contentEnd, end: end, fenceCharacter: fence))
            if contentEnd - start <= Self.maximumLexicalLineLength,
               let marker = Self.fenceMarker(in: units, start: start, end: contentEnd) {
                if fence == nil {
                    fence = marker
                } else if fence == marker {
                    fence = nil
                }
            }
            start = end
        }

        while cursor < units.count {
            if (cursor & 4095) == 0, isCancelled() { return nil }
            let character = units[cursor]
            if Self.isLineBreak(character) {
                let contentEnd = cursor
                cursor += 1
                if character == 13, cursor < units.count, units[cursor] == 10 {
                    cursor += 1
                }
                appendLine(contentEnd: contentEnd, end: cursor)
            } else {
                cursor += 1
            }
        }
        // The final empty line matters to the caret and the gutter after a
        // trailing newline. An empty document still has one editable line.
        appendLine(contentEnd: units.count, end: units.count)
        guard !isCancelled() else { return nil }
        utf16Count = units.count
        lines = result
    }

    func lineIndex(containing offset: Int) -> Int {
        let position = max(0, min(offset, utf16Count))
        var lower = 0
        var upper = lines.count
        while lower < upper {
            let middle = lower + (upper - lower) / 2
            if lines[middle].start <= position {
                lower = middle + 1
            } else {
                upper = middle
            }
        }
        return max(0, lower - 1)
    }

    func lineNumber(startingAt offset: Int) -> Int? {
        let index = lineIndex(containing: offset)
        return lines[index].start == offset ? index + 1 : nil
    }

    /// Include nearby logical lines, but never let an unusually long line or a
    /// tiny-font viewport turn a paint into a whole-document lexical scan.
    /// Partial boundary lines are deliberately left in the base editor style.
    func highlightWindow(around visibleRange: NSRange) -> NSRange {
        guard utf16Count > 0 else { return NSRange(location: 0, length: 0) }
        let visibleStart = min(max(0, visibleRange.location), utf16Count)
        let visibleLength = min(max(0, visibleRange.length), utf16Count - visibleStart)
        let firstVisible = lineIndex(containing: visibleStart)
        let lastVisible = lineIndex(containing: visibleStart + visibleLength)
        let first = max(0, firstVisible - 12)
        let last = min(lines.count - 1, min(first + Self.maximumHighlightLines - 1, lastVisible + 12))
        var start = lines[first].start
        if visibleStart - start > Self.maximumHighlightLength / 4 {
            start = visibleStart - Self.maximumHighlightLength / 4
        }
        let end = min(lines[last].end, start + Self.maximumHighlightLength)
        return NSRange(location: start, length: max(0, end - start))
    }

    private static func isLineBreak(_ character: UInt16) -> Bool {
        character == 10 || character == 13 || character == 0x85
            || character == 0x2028 || character == 0x2029
    }

    private static func fenceMarker(in units: [UInt16], start: Int, end: Int) -> UInt16? {
        var cursor = start
        while cursor < end,
              let scalar = UnicodeScalar(units[cursor]),
              CharacterSet.whitespaces.contains(scalar) {
            cursor += 1
        }
        guard cursor + 2 < end else { return nil }
        let marker = units[cursor]
        guard marker == 96 || marker == 126,
              units[cursor + 1] == marker,
              units[cursor + 2] == marker else { return nil }
        return marker
    }
}
