import Foundation

/// The editor's deliberately lexical `[label](destination)` decoration. One
/// forward scan preserves the previous five styling ranges without repeatedly
/// rescanning suffixes after unmatched opening brackets or parentheses.
enum MarkdownInlineLinkRanges {
    struct Match: Equatable {
        let opening: NSRange
        let label: NSRange
        let separator: NSRange
        let destination: NSRange
        let closing: NSRange

        var range: NSRange {
            NSRange(location: opening.location, length: NSMaxRange(closing) - opening.location)
        }
    }

    static func find(in source: NSString, range: NSRange) -> [Match] {
        guard range.location != NSNotFound,
              range.location >= 0,
              range.location < source.length,
              range.length > 0 else { return [] }
        let length = min(range.length, source.length - range.location)
        let units = Array(source.substring(with: NSRange(location: range.location, length: length)).utf16)
        var matches: [Match] = []
        var opening: Int?
        var cursor = 0
        while cursor < units.count {
            let unit = units[cursor]
            if isLineBreak(unit) {
                opening = nil
                cursor += 1
                continue
            }
            if unit == 91 {
                // '[' is permitted inside the old lexical label grammar. The
                // earliest opener owns the first eligible closing bracket.
                if opening == nil { opening = cursor }
                cursor += 1
                continue
            }
            guard unit == 93 else {
                cursor += 1
                continue
            }
            guard let bracket = opening,
                  cursor + 1 < units.count,
                  units[cursor + 1] == 40 else {
                // No earlier '[' can cross this nonmatching ']' in its label.
                opening = nil
                cursor += 1
                continue
            }

            let destinationStart = cursor + 2
            var closing = destinationStart
            while closing < units.count, units[closing] != 41,
                  !isLineBreak(units[closing]) {
                closing += 1
            }
            if closing < units.count, units[closing] == 41 {
                let first = bracket > 0 && units[bracket - 1] == 33 ? bracket - 1 : bracket
                matches.append(Match(
                    opening: NSRange(location: range.location + first, length: bracket + 1 - first),
                    label: NSRange(location: range.location + bracket + 1, length: cursor - bracket - 1),
                    separator: NSRange(location: range.location + cursor, length: 2),
                    destination: NSRange(location: range.location + destinationStart, length: closing - destinationStart),
                    closing: NSRange(location: range.location + closing, length: 1)
                ))
                cursor = closing + 1
            } else {
                // No ')' before the line boundary means that every later
                // candidate in this suffix also fails; consume the suffix once.
                cursor = closing
            }
            opening = nil
        }
        return matches
    }

    /// Preserve the previous lowercase HTTP(S) lexical grammar, including a
    /// nested '<' within a candidate. A failed candidate ends at whitespace or
    /// the window boundary, so every nested candidate in that consumed suffix
    /// would fail at the same boundary and must not be rescanned.
    static func findAutolinks(in source: NSString, range: NSRange) -> [NSRange] {
        guard range.location != NSNotFound,
              range.location >= 0,
              range.location < source.length,
              range.length > 0 else { return [] }
        let length = min(range.length, source.length - range.location)
        let units = Array(source.substring(with: NSRange(location: range.location, length: length)).utf16)
        var ranges: [NSRange] = []
        var cursor = 0
        while cursor < units.count {
            guard units[cursor] == 60 else {
                cursor += 1
                continue
            }
            let destinationStart: Int
            if hasPrefix(httpPrefix, in: units, at: cursor) {
                destinationStart = cursor + httpPrefix.count
            } else if hasPrefix(httpsPrefix, in: units, at: cursor) {
                destinationStart = cursor + httpsPrefix.count
            } else {
                cursor += 1
                continue
            }
            var end = destinationStart
            while end < units.count, units[end] != 62,
                  !isRegularExpressionWhitespace(units[end]) {
                end += 1
            }
            if end < units.count, units[end] == 62, end > destinationStart {
                ranges.append(NSRange(location: range.location + cursor, length: end + 1 - cursor))
            }
            cursor = end < units.count ? end + 1 : end
        }
        return ranges
    }

    private static let httpPrefix: [UInt16] = [60, 104, 116, 116, 112, 58, 47, 47]
    private static let httpsPrefix: [UInt16] = [60, 104, 116, 116, 112, 115, 58, 47, 47]

    private static func hasPrefix(_ prefix: [UInt16], in units: [UInt16], at index: Int) -> Bool {
        guard prefix.count <= units.count - index else { return false }
        return prefix.indices.allSatisfy { units[index + $0] == prefix[$0] }
    }

    /// ICU's regex \s uses Unicode White_Space. Foundation's whitespace sets
    /// can additionally classify U+200B, which the old lexical pattern accepts
    /// as a destination character. There are no supplementary White_Space
    /// scalars, so UTF-16 code-unit membership is sufficient here.
    private static func isRegularExpressionWhitespace(_ unit: UInt16) -> Bool {
        switch unit {
        case 9 ... 13, 32, 0x85, 0xA0, 0x1680, 0x2000 ... 0x200A,
             0x2028, 0x2029, 0x202F, 0x205F, 0x3000:
            true
        default:
            false
        }
    }

    private static func isLineBreak(_ unit: UInt16) -> Bool {
        unit == 10 || unit == 13 || unit == 0x85 || unit == 0x2028 || unit == 0x2029
    }
}

/// Protection for one bounded lexical line/window, addressed in absolute UTF-16
/// coordinates. Each scanner's matches are nonoverlapping, so checking and
/// marking their slices visits at most the window's units for that phase; it
/// never searches an ever-growing list of previously decorated tokens.
struct MarkdownInlineProtection {
    private let baseOffset: Int
    private var units: [Bool]

    init(range: NSRange) {
        guard range.location != NSNotFound, range.location >= 0, range.length > 0 else {
            baseOffset = 0
            units = []
            return
        }
        baseOffset = range.location
        units = Array(repeating: false, count: range.length)
    }

    func intersects(_ candidate: NSRange) -> Bool {
        guard let local = clippedOffsets(for: candidate) else { return false }
        return units[local].contains(true)
    }

    mutating func insert(_ candidate: NSRange) {
        guard let local = clippedOffsets(for: candidate) else { return }
        for index in local { units[index] = true }
    }

    private func clippedOffsets(for candidate: NSRange) -> Range<Int>? {
        guard candidate.location != NSNotFound, candidate.location >= 0,
              candidate.length > 0, !units.isEmpty else { return nil }
        // Subtraction and clipping avoid ever forming a possibly overflowing
        // NSMaxRange for a candidate that extends beyond this source window.
        if candidate.location < baseOffset {
            let skipped = baseOffset - candidate.location
            guard candidate.length > skipped else { return nil }
            return 0 ..< min(candidate.length - skipped, units.count)
        }
        let start = candidate.location - baseOffset
        guard start < units.count else { return nil }
        return start ..< start + min(candidate.length, units.count - start)
    }
}
