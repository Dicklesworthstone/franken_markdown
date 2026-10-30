import Foundation

/// Lexical decoration ranges for one bounded editor window. Maximal backtick
/// runs are indexed once; matching never repeatedly searches an unmatched
/// suffix. Source parsing and code-span semantics remain owned by the renderer.
enum MarkdownInlineCodeRanges {
    private struct Run {
        let start: Int
        let length: Int
        let line: Int
    }

    static func find(
        in source: NSString,
        range: NSRange,
        maximumLength: Int = 2_200
    ) -> [NSRange] {
        guard range.location != NSNotFound,
              range.location >= 0,
              range.location < source.length,
              range.length > 0,
              maximumLength > 0 else { return [] }
        let length = min(range.length, source.length - range.location)
        // Materialize only the caller's bounded window once, avoiding repeated
        // UTF-16 index conversions for a Swift-backed NSString with Unicode.
        let units = Array(source.substring(with: NSRange(location: range.location, length: length)).utf16)
        var cursor = 0
        var line = 0
        var runs: [Run] = []
        while cursor < units.count {
            let unit = units[cursor]
            if unit == 96 {
                let start = cursor
                repeat { cursor += 1 }
                while cursor < units.count && units[cursor] == 96
                runs.append(Run(start: range.location + start, length: cursor - start, line: line))
            } else {
                if unit == 10 || unit == 13 || unit == 0x85
                    || unit == 0x2028 || unit == 0x2029 {
                    line += 1
                }
                cursor += 1
            }
        }
        guard runs.count > 1 else { return [] }

        // A backwards pass makes every closer lookup constant-time on the
        // forward pass, including thousands of unmatched or unequal runs.
        var next = Array(repeating: -1, count: runs.count)
        var nextByLength: [Int: Int] = [:]
        var followingLine = -1
        for index in runs.indices.reversed() {
            let run = runs[index]
            if run.line != followingLine {
                nextByLength.removeAll(keepingCapacity: false)
                followingLine = run.line
            }
            next[index] = nextByLength[run.length] ?? -1
            nextByLength[run.length] = index
        }

        var ranges: [NSRange] = []
        var index = 0
        while index < runs.count {
            let closer = next[index]
            guard closer >= 0 else {
                index += 1
                continue
            }
            let opening = runs[index]
            let closing = runs[closer]
            let length = closing.start + closing.length - opening.start
            if length <= maximumLength {
                ranges.append(NSRange(location: opening.start, length: length))
            }
            // The earliest matching opener owns enclosed runs even when its
            // decoration is too long. Later token classes retain their normal
            // bounded behavior; no source characters are removed or rewritten.
            index = closer + 1
        }
        return ranges
    }
}
