import SwiftUI
import UIKit

/// Native Markdown source editor whose presentation vocabulary mirrors the
/// FrankenMarkdown website playground. Highlighting is deliberately lexical:
/// the bundled Rust engine remains the sole parser and diagnostic authority.
struct MarkdownCodeEditor: UIViewRepresentable {
    @Binding var text: String
    @Binding var isFocused: Bool

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> MarkdownTextView {
        let view = MarkdownTextView()
        view.delegate = context.coordinator
        view.backgroundColor = .clear
        view.keyboardDismissMode = .interactive
        view.autocapitalizationType = .none
        view.autocorrectionType = .no
        view.smartDashesType = .no
        view.smartQuotesType = .no
        view.spellCheckingType = .no
        view.alwaysBounceVertical = true
        view.showsVerticalScrollIndicator = true
        view.tintColor = UIColor(Lab.emerald)
        view.accessibilityLabel = "Markdown source editor"
        view.accessibilityHint = "Edit Markdown with syntax highlighting. The Rust renderer supplies authoritative diagnostics."
        context.coordinator.applyHighlight(to: view, replacingText: text)
        return view
    }

    func updateUIView(_ view: MarkdownTextView, context: Context) {
        context.coordinator.parent = self
        if view.text != text {
            context.coordinator.applyHighlight(to: view, replacingText: text)
        } else {
            context.coordinator.refreshTypingAttributes(in: view)
        }
        if isFocused, !view.isFirstResponder {
            view.becomeFirstResponder()
        } else if !isFocused, view.isFirstResponder {
            view.resignFirstResponder()
        }
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownCodeEditor
        private var isApplyingHighlight = false

        init(_ parent: MarkdownCodeEditor) { self.parent = parent }

        func textViewDidBeginEditing(_ textView: UITextView) { parent.isFocused = true }
        func textViewDidEndEditing(_ textView: UITextView) { parent.isFocused = false }

        func textViewDidChange(_ textView: UITextView) {
            guard !isApplyingHighlight, let view = textView as? MarkdownTextView else { return }
            parent.text = view.text
            applyHighlight(to: view, replacingText: nil)
        }

        func textViewDidChangeSelection(_ textView: UITextView) { textView.setNeedsDisplay() }
        func scrollViewDidScroll(_ scrollView: UIScrollView) { scrollView.setNeedsDisplay() }

        func applyHighlight(to view: MarkdownTextView, replacingText replacement: String?) {
            isApplyingHighlight = true
            let selectedRange = view.selectedRange
            let contentOffset = view.contentOffset
            if let replacement { view.text = replacement }
            let source = view.text ?? ""
            let baseFont = UIFontMetrics(forTextStyle: .body).scaledFont(
                for: .monospacedSystemFont(ofSize: 15, weight: .regular)
            )
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = 4
            paragraph.paragraphSpacing = 1
            let storage = NSMutableAttributedString(
                string: source,
                attributes: [
                    .font: baseFont,
                    .foregroundColor: UIColor(red: 0.72, green: 0.81, blue: 0.76, alpha: 1),
                    .paragraphStyle: paragraph
                ]
            )
            MarkdownLexicalHighlighter.highlight(source, storage: storage)
            view.attributedText = storage
            let length = (source as NSString).length
            let location = min(selectedRange.location, length)
            view.selectedRange = NSRange(
                location: location,
                length: min(selectedRange.length, length - location)
            )
            refreshTypingAttributes(in: view)
            view.setContentOffset(contentOffset, animated: false)
            view.setNeedsDisplay()
            isApplyingHighlight = false
        }

        func refreshTypingAttributes(in view: UITextView) {
            view.typingAttributes = [
                .font: UIFontMetrics(forTextStyle: .body).scaledFont(
                    for: .monospacedSystemFont(ofSize: 15, weight: .regular)
                ),
                .foregroundColor: UIColor(red: 0.72, green: 0.81, blue: 0.76, alpha: 1)
            ]
        }
    }
}

private enum MarkdownLexicalHighlighter {
    private static let marker = UIColor(red: 0.31, green: 0.44, blue: 0.37, alpha: 1)
    private static let heading = UIColor(Lab.emerald)
    private static let bold = UIColor(red: 0.97, green: 0.98, blue: 0.99, alpha: 1)
    private static let emphasis = UIColor(red: 0.84, green: 0.91, blue: 0.87, alpha: 1)
    private static let amber = UIColor(Lab.amber)
    private static let codeBlock = UIColor(red: 0.62, green: 0.75, blue: 0.68, alpha: 1)
    private static let link = UIColor(red: 0.43, green: 0.91, blue: 0.72, alpha: 1)
    private static let quote = UIColor(red: 0.56, green: 0.70, blue: 0.63, alpha: 1)

    static func highlight(_ source: String, storage: NSMutableAttributedString) {
        let nsSource = source as NSString
        guard nsSource.length > 0 else { return }
        var location = 0
        var fenceCharacter: unichar?

        while location < nsSource.length {
            let rawLineRange = nsSource.lineRange(for: NSRange(location: location, length: 0))
            let lineLength = rawLineRange.length
                - ((rawLineRange.length > 0 && nsSource.character(at: NSMaxRange(rawLineRange) - 1) == 10) ? 1 : 0)
            let lineRange = NSRange(location: rawLineRange.location, length: max(0, lineLength))
            let line = nsSource.substring(with: lineRange)

            // Mirrors the website's adversarial-input bound: a pathological
            // 20k-character line stays plain rather than monopolizing typing.
            if lineRange.length > 20_000 {
                location = NSMaxRange(rawLineRange)
                continue
            }

            if let fence = firstMatch(fenceRegex, in: source, range: lineRange) {
                let markerRange = fence.range(at: 2)
                apply(color: marker, to: storage, range: markerRange)
                if fenceCharacter == nil {
                    fenceCharacter = nsSource.character(at: markerRange.location)
                    apply(color: heading, font: font(.semibold), to: storage, range: fence.range(at: 3))
                } else if nsSource.character(at: markerRange.location) == fenceCharacter {
                    fenceCharacter = nil
                } else {
                    apply(color: codeBlock, to: storage, range: lineRange)
                }
                location = NSMaxRange(rawLineRange)
                continue
            }

            if fenceCharacter != nil {
                apply(color: codeBlock, to: storage, range: lineRange)
                location = NSMaxRange(rawLineRange)
                continue
            }

            if let match = firstMatch(headingRegex, in: source, range: lineRange) {
                apply(color: marker, to: storage, range: match.range(at: 2))
                apply(color: heading, font: font(.bold), to: storage, range: match.range(at: 4))
            } else if setextRegex.firstMatch(in: source, range: lineRange) != nil
                        || horizontalRuleRegex.firstMatch(in: source, range: lineRange) != nil {
                apply(color: marker, font: font(.bold), to: storage, range: lineRange)
            } else if let match = firstMatch(blockquoteRegex, in: source, range: lineRange) {
                apply(color: marker, to: storage, range: match.range(at: 1))
                let content = match.range(at: 2)
                apply(color: quote, font: italicFont(), to: storage, range: content)
                highlightInline(source, storage: storage, range: content)
            } else if let match = firstMatch(listRegex, in: source, range: lineRange) {
                apply(color: marker, to: storage, range: match.range(at: 2))
                if match.range(at: 4).location != NSNotFound {
                    apply(color: amber, font: font(.semibold), to: storage, range: match.range(at: 4))
                }
                highlightInline(source, storage: storage, range: match.range(at: 5))
            } else if let match = firstMatch(referenceRegex, in: source, range: lineRange) {
                apply(color: marker, to: storage, range: match.range(at: 1))
                apply(color: link, underline: true, to: storage, range: match.range(at: 2))
                apply(color: marker, to: storage, range: match.range(at: 3))
                apply(color: marker, to: storage, range: match.range(at: 4))
                apply(color: emphasis, font: italicFont(), to: storage, range: match.range(at: 5))
            } else if line.contains("|") {
                highlightTable(source, storage: storage, range: lineRange)
            } else {
                highlightInline(source, storage: storage, range: lineRange)
            }

            location = NSMaxRange(rawLineRange)
        }
    }

    private static func highlightTable(_ source: String, storage: NSMutableAttributedString, range: NSRange) {
        let nsSource = source as NSString
        var allMarkers = true
        for index in range.location ..< NSMaxRange(range) {
            let character = nsSource.character(at: index)
            if character != 32, character != 9, character != 124, character != 58, character != 45 {
                allMarkers = false
                break
            }
        }
        if allMarkers {
            apply(color: marker, to: storage, range: range)
            return
        }
        for index in range.location ..< NSMaxRange(range) where nsSource.character(at: index) == 124 {
            apply(color: marker, to: storage, range: NSRange(location: index, length: 1))
        }
        highlightInline(source, storage: storage, range: range)
    }

    private static func highlightInline(_ source: String, storage: NSMutableAttributedString, range: NSRange) {
        var protected: [NSRange] = []

        enumerate(inlineCodeRegex, source: source, range: range) { match in
            guard match.range.length <= 2_200 else { return }
            apply(color: amber, to: storage, range: match.range)
            protected.append(match.range)
        }
        enumerate(linkRegex, source: source, range: range) { match in
            guard !intersects(match.range, protected) else { return }
            apply(color: marker, to: storage, range: match.range(at: 1))
            apply(color: link, underline: true, to: storage, range: match.range(at: 2))
            apply(color: marker, to: storage, range: match.range(at: 3))
            apply(color: marker, to: storage, range: match.range(at: 4))
            apply(color: marker, to: storage, range: match.range(at: 5))
            protected.append(match.range)
        }
        enumerate(autolinkRegex, source: source, range: range) { match in
            guard !intersects(match.range, protected) else { return }
            apply(color: link, underline: true, to: storage, range: match.range)
            protected.append(match.range)
        }
        enumerate(boldRegex, source: source, range: range) { match in
            guard match.range.length <= 220, !intersects(match.range, protected) else { return }
            apply(color: marker, to: storage, range: match.range(at: 1))
            apply(color: bold, font: font(.bold), to: storage, range: match.range(at: 2))
            apply(color: marker, to: storage, range: match.range(at: 3))
            protected.append(match.range)
        }
        enumerate(emphasisRegex, source: source, range: range) { match in
            guard match.range.length <= 160, !intersects(match.range, protected) else { return }
            apply(color: marker, to: storage, range: match.range(at: 1))
            apply(color: emphasis, font: italicFont(), to: storage, range: match.range(at: 2))
            apply(color: marker, to: storage, range: match.range(at: 3))
            protected.append(match.range)
        }
        enumerate(strikeRegex, source: source, range: range) { match in
            guard match.range.length <= 160, !intersects(match.range, protected) else { return }
            apply(color: marker, to: storage, range: match.range(at: 1))
            storage.addAttributes([
                .foregroundColor: emphasis,
                .strikethroughStyle: NSUnderlineStyle.single.rawValue
            ], range: match.range(at: 2))
            apply(color: marker, to: storage, range: match.range(at: 3))
        }
    }

    private static func apply(
        color: UIColor,
        font: UIFont? = nil,
        underline: Bool = false,
        to storage: NSMutableAttributedString,
        range: NSRange
    ) {
        guard range.location != NSNotFound, range.length > 0 else { return }
        storage.addAttribute(.foregroundColor, value: color, range: range)
        if let font { storage.addAttribute(.font, value: font, range: range) }
        if underline { storage.addAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, range: range) }
    }

    private static func font(_ weight: UIFont.Weight) -> UIFont {
        UIFontMetrics(forTextStyle: .body).scaledFont(
            for: .monospacedSystemFont(ofSize: 15, weight: weight)
        )
    }

    private static func italicFont() -> UIFont {
        let base = UIFont.monospacedSystemFont(ofSize: 15, weight: .regular)
        let descriptor = base.fontDescriptor.withSymbolicTraits(.traitItalic) ?? base.fontDescriptor
        return UIFontMetrics(forTextStyle: .body).scaledFont(for: UIFont(descriptor: descriptor, size: 15))
    }

    private static func intersects(_ range: NSRange, _ protected: [NSRange]) -> Bool {
        protected.contains { NSIntersectionRange(range, $0).length > 0 }
    }

    private static func enumerate(
        _ regex: NSRegularExpression,
        source: String,
        range: NSRange,
        body: (NSTextCheckingResult) -> Void
    ) {
        regex.enumerateMatches(in: source, range: range) { match, _, _ in
            if let match { body(match) }
        }
    }

    private static func firstMatch(
        _ regex: NSRegularExpression,
        in source: String,
        range: NSRange
    ) -> NSTextCheckingResult? {
        regex.firstMatch(in: source, range: range)
    }

    private static func regex(_ pattern: String, options: NSRegularExpression.Options = []) -> NSRegularExpression {
        // Every expression is a literal bundled with the app and covered by the
        // Xcode build. A fallback that silently drops a token class would hide
        // an implementation error, so fail loudly in development.
        try! NSRegularExpression(pattern: pattern, options: options)
    }

    private static let fenceRegex = regex(#"^(\s*)(```+|~~~+)(.*)$"#)
    private static let headingRegex = regex(#"^(\s{0,3})(#{1,6})(\s+)(.*)$"#)
    private static let setextRegex = regex(#"^\s{0,3}(?:=+|-{2,})\s*$"#)
    private static let horizontalRuleRegex = regex(#"^\s{0,3}(?:(?:\*\s*){3,}|(?:-\s*){3,}|(?:_\s*){3,})$"#)
    private static let blockquoteRegex = regex(#"^(\s{0,3}(?:>\s?)+)(.*)$"#)
    private static let listRegex = regex(#"^(\s*)([-+*]|\d{1,9}[.)])(\s+)(\[[ xX]\]\s+)?(.*)$"#)
    private static let referenceRegex = regex(#"^(\s{0,3}\[)([^\]]+)(\]:\s*)(\S+)(.*)$"#)
    private static let inlineCodeRegex = regex(#"(`+)([^`\n]|[^`\n][\s\S]*?[^`\n])\1(?!`)"#)
    private static let linkRegex = regex(#"(!?\[)([^\]\n]*)(\]\()([^)\n]*)(\))"#)
    private static let autolinkRegex = regex(#"<https?://[^>\s]+>"#)
    private static let boldRegex = regex(#"(\*\*|__)(?!\s)([\s\S]*?\S)(\*\*|__)"#)
    private static let emphasisRegex = regex(#"(\*|_)(?![\s*_])([^*_\n]*?\S)(\*|_)"#)
    private static let strikeRegex = regex(#"(~~)(?!\s)([\s\S]*?\S)(~~)"#)
}

final class MarkdownTextView: UITextView {
    private let gutterWidth: CGFloat = 42

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        textContainerInset = UIEdgeInsets(top: 14, left: gutterWidth + 12, bottom: 18, right: 14)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func draw(_ rect: CGRect) {
        drawCurrentLine()
        super.draw(rect)
        drawGutter()
    }

    private func drawCurrentLine() {
        let length = (text as NSString).length
        guard length > 0 else { return }
        let glyph = layoutManager.glyphIndexForCharacter(at: min(selectedRange.location, length - 1))
        let fragment = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let y = fragment.minY + textContainerInset.top - contentOffset.y
        let row = CGRect(x: gutterWidth, y: y, width: bounds.width - gutterWidth, height: fragment.height)
        UIColor(Lab.emerald).withAlphaComponent(0.055).setFill()
        UIBezierPath(rect: row).fill()
    }

    private func drawGutter() {
        let context = UIGraphicsGetCurrentContext()
        context?.saveGState()
        UIColor(Lab.emerald).withAlphaComponent(0.16).setStroke()
        let divider = UIBezierPath()
        divider.move(to: CGPoint(x: gutterWidth, y: 0))
        divider.addLine(to: CGPoint(x: gutterWidth, y: bounds.height))
        divider.lineWidth = 0.7
        divider.stroke()

        let nsText = text as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: UIFont.monospacedDigitSystemFont(ofSize: 10, weight: .medium),
            .foregroundColor: UIColor(Lab.secondary).withAlphaComponent(0.58)
        ]
        if nsText.length == 0 {
            drawLineNumber(1, y: textContainerInset.top - contentOffset.y + 2, attributes: attributes)
            context?.restoreGState()
            return
        }

        var line = 1
        var start = 0
        while start < nsText.length {
            let glyph = layoutManager.glyphIndexForCharacter(at: start)
            let fragment = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            let y = fragment.minY + textContainerInset.top - contentOffset.y + 2
            if y > -20, y < bounds.height + 20 {
                drawLineNumber(line, y: y, attributes: attributes)
            }
            let lineRange = nsText.lineRange(for: NSRange(location: start, length: 0))
            start = NSMaxRange(lineRange)
            line += 1
        }
        context?.restoreGState()
    }

    private func drawLineNumber(
        _ line: Int,
        y: CGFloat,
        attributes: [NSAttributedString.Key: Any]
    ) {
        let label = "\(line)" as NSString
        let size = label.size(withAttributes: attributes)
        label.draw(at: CGPoint(x: gutterWidth - size.width - 8, y: y), withAttributes: attributes)
    }
}
