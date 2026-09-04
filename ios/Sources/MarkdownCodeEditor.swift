import SwiftUI
import UIKit

/// Native Markdown source editor whose presentation vocabulary mirrors the
/// FrankenMarkdown website playground. Highlighting is deliberately lexical:
/// the bundled Rust engine remains the sole parser and diagnostic authority.
struct MarkdownCodeEditor: UIViewRepresentable {
    @Binding var text: String
    @Binding var isFocused: Bool
    @Environment(\.colorScheme) private var colorScheme
    @AppStorage(Lab.textScaleStorageKey) private var uiTextScale = Lab.defaultTextScale

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> MarkdownEditorContainer {
        let container = MarkdownEditorContainer()
        let view = container.textView
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
        context.coordinator.lastTextScale = Lab.clampedTextScale(uiTextScale)
        context.coordinator.applyHighlight(to: view, replacingText: text)
        return container
    }

    func updateUIView(_ container: MarkdownEditorContainer, context: Context) {
        let view = container.textView
        context.coordinator.parent = self
        let clampedTextScale = Lab.clampedTextScale(uiTextScale)
        if view.text != text
            || context.coordinator.lastColorScheme != colorScheme
            || context.coordinator.lastTextScale != clampedTextScale {
            context.coordinator.lastColorScheme = colorScheme
            context.coordinator.lastTextScale = clampedTextScale
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
        var lastColorScheme: ColorScheme?
        var lastTextScale: Double?
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
        func scrollViewDidScroll(_ scrollView: UIScrollView) {
            scrollView.setNeedsDisplay()
            (scrollView as? MarkdownTextView)?.gutterView?.setNeedsDisplay()
        }

        func applyHighlight(to view: MarkdownTextView, replacingText replacement: String?) {
            isApplyingHighlight = true
            let selectedRange = view.selectedRange
            let contentOffset = view.contentOffset
            if let replacement { view.text = replacement }
            let source = view.text ?? ""
            let baseFont = UIFont.monospacedSystemFont(ofSize: Lab.size(15), weight: .regular)
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = 4
            paragraph.paragraphSpacing = 1
            let storage = NSMutableAttributedString(
                string: source,
                attributes: [
                    .font: baseFont,
                    .foregroundColor: UIColor(Lab.text),
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
            view.gutterView?.setNeedsDisplay()
            isApplyingHighlight = false
        }

        func refreshTypingAttributes(in view: UITextView) {
            view.typingAttributes = [
                .font: UIFont.monospacedSystemFont(ofSize: Lab.size(15), weight: .regular),
                .foregroundColor: UIColor(Lab.text)
            ]
        }
    }
}

private enum MarkdownLexicalHighlighter {
    private static let marker = UIColor(Lab.secondary)
    private static let heading = UIColor(Lab.emerald)
    private static let bold = UIColor(Lab.text)
    private static let emphasis = UIColor(Lab.secondary)
    private static let amber = UIColor(Lab.amber)
    private static let codeBlock = UIColor(Lab.secondary)
    private static let link = UIColor(Lab.cyan)
    private static let quote = UIColor(Lab.secondary)

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
            } else if line.trimmingCharacters(in: .whitespaces).hasPrefix("|")
                        && line.trimmingCharacters(in: .whitespaces).hasSuffix("|") {
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
            let closing = NSRange(
                location: NSMaxRange(match.range) - match.range(at: 1).length,
                length: match.range(at: 1).length
            )
            apply(color: marker, to: storage, range: closing)
            protected.append(match.range)
        }
        enumerate(emphasisRegex, source: source, range: range) { match in
            guard match.range.length <= 160, !intersects(match.range, protected) else { return }
            apply(color: marker, to: storage, range: match.range(at: 1))
            apply(color: emphasis, font: italicFont(), to: storage, range: match.range(at: 2))
            let closing = NSRange(
                location: NSMaxRange(match.range) - match.range(at: 1).length,
                length: match.range(at: 1).length
            )
            apply(color: marker, to: storage, range: closing)
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
        UIFont.monospacedSystemFont(ofSize: Lab.size(15), weight: weight)
    }

    private static func italicFont() -> UIFont {
        let size = Lab.size(15)
        let base = UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
        let descriptor = base.fontDescriptor.withSymbolicTraits(.traitItalic) ?? base.fontDescriptor
        return UIFont(descriptor: descriptor, size: size)
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
    private static let boldRegex = regex(#"(\*\*|__)(?!\s)([\s\S]*?\S)\1"#)
    private static let emphasisRegex = regex(#"(\*|_)(?![\s*_])([^*_\n]*?\S)\1"#)
    private static let strikeRegex = regex(#"(~~)(?!\s)([\s\S]*?\S)(~~)"#)
}

final class MarkdownEditorContainer: UIView {
    static var gutterWidth: CGFloat { max(44, Lab.size(32)) }

    let textView = MarkdownTextView()
    private let gutterView = MarkdownLineNumberView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        gutterView.textView = textView
        textView.gutterView = gutterView
        addSubview(gutterView)
        addSubview(textView)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func layoutSubviews() {
        super.layoutSubviews()
        gutterView.frame = CGRect(x: 0, y: 0, width: Self.gutterWidth, height: bounds.height)
        textView.frame = CGRect(
            x: Self.gutterWidth,
            y: 0,
            width: max(0, bounds.width - Self.gutterWidth),
            height: bounds.height
        )
        gutterView.setNeedsDisplay()
    }
}

final class MarkdownTextView: UITextView {
    weak var gutterView: UIView?

    private var editorInsets: UIEdgeInsets {
        UIEdgeInsets(top: 14, left: 12, bottom: 18, right: 14)
    }

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        textContainerInset = editorInsets
        self.textContainer.widthTracksTextView = true
        contentInsetAdjustmentBehavior = .never
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func layoutSubviews() {
        super.layoutSubviews()

        // Catalyst can restore UITextView defaults while swapping layout modes
        // during a live window resize. Keep the text metrics deterministic.
        if textContainerInset != editorInsets {
            textContainerInset = editorInsets
        }
        textContainer.widthTracksTextView = true
        layoutManager.ensureLayout(for: textContainer)
        gutterView?.setNeedsDisplay()
    }

    override func draw(_ rect: CGRect) {
        drawCurrentLine()
        super.draw(rect)
    }

    private func drawCurrentLine() {
        let length = (text as NSString).length
        guard length > 0 else { return }
        layoutManager.ensureLayout(for: textContainer)
        let glyph = layoutManager.glyphIndexForCharacter(at: min(selectedRange.location, length - 1))
        let fragment = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let y = fragment.minY + textContainerInset.top
        let row = CGRect(x: bounds.minX, y: y, width: bounds.width, height: fragment.height)
        UIColor(Lab.emerald).withAlphaComponent(0.055).setFill()
        UIBezierPath(rect: row).fill()
    }
}

private final class MarkdownLineNumberView: UIView {
    weak var textView: MarkdownTextView?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isUserInteractionEnabled = false
        contentMode = .redraw
        accessibilityElementsHidden = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func draw(_ rect: CGRect) {
        guard let textView else { return }
        let context = UIGraphicsGetCurrentContext()
        context?.saveGState()
        UIColor(Lab.emerald).withAlphaComponent(0.16).setStroke()
        let divider = UIBezierPath()
        divider.move(to: CGPoint(x: bounds.maxX - 0.5, y: bounds.minY))
        divider.addLine(to: CGPoint(x: bounds.maxX - 0.5, y: bounds.maxY))
        divider.lineWidth = 0.7
        divider.stroke()

        let nsText = textView.text as NSString
        let numberFont = UIFont.monospacedDigitSystemFont(ofSize: Lab.size(10), weight: .medium)
        let attributes: [NSAttributedString.Key: Any] = [
            .font: numberFont,
            .foregroundColor: UIColor(Lab.secondary).withAlphaComponent(0.58)
        ]
        if nsText.length == 0 {
            drawLineNumber(1, y: textView.textContainerInset.top, attributes: attributes)
            context?.restoreGState()
            return
        }

        textView.layoutManager.ensureLayout(for: textView.textContainer)
        let lineStarts = logicalLineStarts(in: nsText)
        let glyphRange = textView.layoutManager.glyphRange(for: textView.textContainer)
        textView.layoutManager.enumerateLineFragments(forGlyphRange: glyphRange) {
            [weak self] _, usedRect, _, fragmentGlyphRange, _ in
            guard let self, let textView = self.textView else { return }
            let characterRange = textView.layoutManager.characterRange(
                forGlyphRange: fragmentGlyphRange,
                actualGlyphRange: nil
            )
            guard let line = lineStarts[characterRange.location] else {
                // A soft-wrapped continuation deliberately has no number.
                return
            }
            let y = usedRect.minY - textView.contentOffset.y
                + max(0, (usedRect.height - numberFont.lineHeight) * 0.5)
            if y > -20, y < self.bounds.height + 20 {
                self.drawLineNumber(line, y: y, attributes: attributes)
            }
        }

        if nsText.character(at: nsText.length - 1) == 10,
           let trailingLine = lineStarts[nsText.length] {
            let fragment = textView.layoutManager.extraLineFragmentRect
            let y = fragment.minY - textView.contentOffset.y
            if fragment != .zero, y > -20, y < bounds.height + 20 {
                drawLineNumber(trailingLine, y: y, attributes: attributes)
            }
        }
        context?.restoreGState()
    }

    private func logicalLineStarts(in text: NSString) -> [Int: Int] {
        var result = [0: 1]
        var line = 1
        var cursor = 0
        while cursor < text.length {
            let range = text.lineRange(for: NSRange(location: cursor, length: 0))
            let next = NSMaxRange(range)
            guard next > cursor else { break }
            cursor = next
            if cursor <= text.length {
                line += 1
                result[cursor] = line
            }
        }
        return result
    }

    private func drawLineNumber(
        _ line: Int,
        y: CGFloat,
        attributes: [NSAttributedString.Key: Any]
    ) {
        let label = "\(line)" as NSString
        let size = label.size(withAttributes: attributes)
        label.draw(at: CGPoint(x: bounds.maxX - size.width - 8, y: y), withAttributes: attributes)
    }
}
