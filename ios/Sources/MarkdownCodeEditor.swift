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
        context.coordinator.lastColorScheme = colorScheme
        context.coordinator.lastTextScale = Lab.clampedTextScale(uiTextScale)
        view.viewportDidChange = { [weak coordinator = context.coordinator, weak view] in
            guard let view else { return }
            coordinator?.scheduleHighlight(in: view)
        }
        context.coordinator.updatePresentation(in: view, replacingText: text)
        return container
    }

    func updateUIView(_ container: MarkdownEditorContainer, context: Context) {
        let view = container.textView
        context.coordinator.parent = self
        let clampedTextScale = Lab.clampedTextScale(uiTextScale)
        if context.coordinator.displayedSource != text
            || context.coordinator.lastColorScheme != colorScheme
            || context.coordinator.lastTextScale != clampedTextScale {
            context.coordinator.lastColorScheme = colorScheme
            context.coordinator.lastTextScale = clampedTextScale
            context.coordinator.updatePresentation(in: view, replacingText: text)
        } else {
            context.coordinator.refreshTypingAttributes(in: view)
        }
        if isFocused, !view.isFirstResponder {
            view.becomeFirstResponder()
        } else if !isFocused, view.isFirstResponder {
            view.resignFirstResponder()
        }
    }

    static func dismantleUIView(_ container: MarkdownEditorContainer, coordinator: Coordinator) {
        coordinator.cancelPendingWork()
        container.textView.viewportDidChange = nil
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownCodeEditor
        var lastColorScheme: ColorScheme?
        var lastTextScale: Double?
        private(set) var displayedSource = ""
        private var sourceGeneration = 0
        private var isApplyingHighlight = false
        private var indexTask: Task<Void, Never>?
        private var indexWorker: Task<MarkdownEditorLineIndex?, Never>?
        private var highlightTask: Task<Void, Never>?
        private var highlightedRange: NSRange?

        init(_ parent: MarkdownCodeEditor) { self.parent = parent }

        deinit {
            indexTask?.cancel()
            indexWorker?.cancel()
            highlightTask?.cancel()
        }

        func cancelPendingWork() {
            sourceGeneration += 1
            indexTask?.cancel()
            indexWorker?.cancel()
            highlightTask?.cancel()
        }

        func textViewDidBeginEditing(_ textView: UITextView) { parent.isFocused = true }
        func textViewDidEndEditing(_ textView: UITextView) { parent.isFocused = false }

        func textViewDidChange(_ textView: UITextView) {
            guard !isApplyingHighlight, let view = textView as? MarkdownTextView else { return }
            let source = view.text ?? ""
            displayedSource = source
            parent.text = source
            refreshTypingAttributes(in: view)
            scheduleIndex(for: source, in: view, debounce: true)
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            textView.setNeedsDisplay()
            if let view = textView as? MarkdownTextView { scheduleHighlight(in: view) }
        }

        func scrollViewDidScroll(_ scrollView: UIScrollView) {
            scrollView.setNeedsDisplay()
            guard let view = scrollView as? MarkdownTextView else { return }
            view.gutterView?.setNeedsDisplay()
            scheduleHighlight(in: view)
        }

        private var baseAttributes: [NSAttributedString.Key: Any] {
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = 4
            paragraph.paragraphSpacing = 1
            return [
                .font: UIFont.monospacedSystemFont(ofSize: Lab.size(15), weight: .regular),
                .foregroundColor: UIColor(Lab.text),
                .paragraphStyle: paragraph
            ]
        }

        /// Replace characters only for an external source change. Lexical
        /// decoration never assigns attributedText, so typing/undo/IME ownership
        /// stays with UITextView and cannot reset the entire document each key.
        func updatePresentation(in view: MarkdownTextView, replacingText replacement: String) {
            isApplyingHighlight = true
            let selectedRange = view.selectedRange
            let contentOffset = view.contentOffset
            let sourceChanged = displayedSource != replacement || view.lineIndex == nil
            if displayedSource != replacement || view.textStorage.length == 0 {
                view.text = replacement
                displayedSource = replacement
            }
            view.textStorage.setAttributes(
                baseAttributes,
                range: NSRange(location: 0, length: view.textStorage.length)
            )
            highlightedRange = nil
            let location = min(selectedRange.location, view.textStorage.length)
            view.selectedRange = NSRange(
                location: location,
                length: min(selectedRange.length, view.textStorage.length - location)
            )
            refreshTypingAttributes(in: view)
            view.setContentOffset(contentOffset, animated: false)
            isApplyingHighlight = false
            if sourceChanged {
                scheduleIndex(for: replacement, in: view, debounce: false)
            } else {
                scheduleHighlight(in: view)
            }
            view.setNeedsDisplay()
            view.gutterView?.setNeedsDisplay()
        }

        private func scheduleIndex(for source: String, in view: MarkdownTextView, debounce: Bool) {
            sourceGeneration += 1
            let generation = sourceGeneration
            indexTask?.cancel()
            indexWorker?.cancel()
            highlightTask?.cancel()
            highlightedRange = nil
            view.lineIndex = nil
            view.gutterView?.setNeedsDisplay()
            indexTask = Task { @MainActor [weak self, weak view] in
                if debounce {
                    do { try await Task.sleep(for: .milliseconds(40)) }
                    catch { return }
                }
                guard !Task.isCancelled, let self, let view,
                      self.sourceGeneration == generation else { return }
                let worker = Task.detached(priority: .utility) {
                    MarkdownEditorLineIndex(source: source, isCancelled: { Task.isCancelled })
                }
                self.indexWorker = worker
                guard let index = await worker.value,
                      !Task.isCancelled, self.sourceGeneration == generation else { return }
                view.lineIndex = index
                view.gutterView?.setNeedsDisplay()
                self.scheduleHighlight(in: view)
            }
        }

        func scheduleHighlight(in view: MarkdownTextView) {
            guard !isApplyingHighlight else { return }
            highlightTask?.cancel()
            highlightTask = Task { @MainActor [weak self, weak view] in
                await Task.yield()
                guard !Task.isCancelled, let self, let view else { return }
                self.highlightVisibleSource(in: view)
            }
        }

        private func highlightVisibleSource(in view: MarkdownTextView) {
            guard view.markedTextRange == nil,
                  let index = view.lineIndex,
                  index.utf16Count == view.textStorage.length,
                  view.bounds.width > 0, view.bounds.height > 0 else { return }
            let visibleRange = view.visibleCharacterRange()
            var range = index.highlightWindow(around: visibleRange)
            guard range.length > 0 else { return }
            let source = view.textStorage.mutableString
            // Keep UTF-16 surrogate pairs together at a capped window edge.
            if range.location > 0, (0xDC00...0xDFFF).contains(source.character(at: range.location)) {
                range.location += 1
                range.length -= 1
            }
            let end = NSMaxRange(range)
            if end < source.length, range.length > 0,
               (0xDC00...0xDFFF).contains(source.character(at: end)) {
                range.length -= 1
            }
            guard range.length > 0, range != highlightedRange else { return }
            let firstLine = index.lines[index.lineIndex(containing: range.location)]
            let lastLine = index.lines[index.lineIndex(containing: NSMaxRange(range) - 1)]
            let windowSource = source.substring(with: range)
            let window = NSMutableAttributedString(string: windowSource, attributes: baseAttributes)
            MarkdownLexicalHighlighter.highlight(
                windowSource,
                storage: window,
                initialFenceCharacter: firstLine.fenceCharacter,
                skipFirstLine: range.location != firstLine.start,
                skipLastLine: NSMaxRange(range) < lastLine.contentEnd
            )

            isApplyingHighlight = true
            let undoManager = view.undoManager
            let wasRegisteringUndo = undoManager?.isUndoRegistrationEnabled == true
            if wasRegisteringUndo { undoManager?.disableUndoRegistration() }
            view.textStorage.beginEditing()
            window.enumerateAttributes(in: NSRange(location: 0, length: window.length)) { attributes, local, _ in
                view.textStorage.setAttributes(
                    attributes,
                    range: NSRange(location: range.location + local.location, length: local.length)
                )
            }
            view.textStorage.endEditing()
            if wasRegisteringUndo { undoManager?.enableUndoRegistration() }
            highlightedRange = range
            refreshTypingAttributes(in: view)
            isApplyingHighlight = false
            view.gutterView?.setNeedsDisplay()
        }

        func refreshTypingAttributes(in view: UITextView) {
            view.typingAttributes = baseAttributes
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

    static func highlight(
        _ source: String,
        storage: NSMutableAttributedString,
        initialFenceCharacter: unichar? = nil,
        skipFirstLine: Bool = false,
        skipLastLine: Bool = false
    ) {
        let nsSource = source as NSString
        guard nsSource.length > 0 else { return }
        var location = 0
        var fenceCharacter = initialFenceCharacter

        while location < nsSource.length {
            let rawLineRange = nsSource.lineRange(for: NSRange(location: location, length: 0))
            let lineLength = rawLineRange.length
                - ((rawLineRange.length > 0 && nsSource.character(at: NSMaxRange(rawLineRange) - 1) == 10) ? 1 : 0)
            let lineRange = NSRange(location: rawLineRange.location, length: max(0, lineLength))
            let line = nsSource.substring(with: lineRange)

            // Mirrors the website's adversarial-input bound: a pathological
            // 20k-character line stays plain rather than monopolizing typing.
            if lineRange.length > MarkdownEditorLineIndex.maximumLexicalLineLength
                || (skipFirstLine && location == 0)
                || (skipLastLine && NSMaxRange(rawLineRange) == nsSource.length) {
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
    var viewportDidChange: (() -> Void)?
    var lineIndex: MarkdownEditorLineIndex?

    private var editorInsets: UIEdgeInsets {
        UIEdgeInsets(top: 14, left: 12, bottom: 18, right: 14)
    }

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        textContainerInset = editorInsets
        self.textContainer.widthTracksTextView = true
        // A far-away viewport must not force layout of every preceding line.
        layoutManager.allowsNonContiguousLayout = true
        contentInsetAdjustmentBehavior = .never
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func layoutSubviews() {
        super.layoutSubviews()
        if textContainerInset != editorInsets {
            textContainerInset = editorInsets
        }
        if !textContainer.widthTracksTextView {
            textContainer.widthTracksTextView = true
        }
        gutterView?.setNeedsDisplay()
        viewportDidChange?()
    }

    func visibleGlyphRange() -> NSRange {
        guard textStorage.length > 0, bounds.width > 0, bounds.height > 0 else {
            return NSRange(location: 0, length: 0)
        }
        let viewport = CGRect(
            x: bounds.minX - textContainerInset.left,
            y: bounds.minY - textContainerInset.top,
            width: bounds.width,
            height: bounds.height
        )
        layoutManager.ensureLayout(forBoundingRect: viewport, in: textContainer)
        return layoutManager.glyphRange(forBoundingRect: viewport, in: textContainer)
    }

    func visibleCharacterRange() -> NSRange {
        let glyphs = visibleGlyphRange()
        guard glyphs.length > 0 else { return NSRange(location: 0, length: 0) }
        return layoutManager.characterRange(forGlyphRange: glyphs, actualGlyphRange: nil)
    }

    override func draw(_ rect: CGRect) {
        drawCurrentLine()
        super.draw(rect)
    }

    private func drawCurrentLine() {
        guard textStorage.length > 0, let selection = selectedTextRange else { return }
        let visible = visibleCharacterRange()
        guard selectedRange.location >= visible.location,
              selectedRange.location <= NSMaxRange(visible) else { return }
        let caret = caretRect(for: selection.start)
        guard caret.intersects(bounds) else { return }
        let row = CGRect(
            x: bounds.minX,
            y: caret.minY,
            width: bounds.width,
            height: max(caret.height, font?.lineHeight ?? 0)
        )
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

        let numberFont = UIFont.monospacedDigitSystemFont(ofSize: Lab.size(10), weight: .medium)
        let attributes: [NSAttributedString.Key: Any] = [
            .font: numberFont,
            .foregroundColor: UIColor(Lab.secondary).withAlphaComponent(0.58)
        ]
        if textView.textStorage.length == 0 {
            drawLineNumber(1, y: textView.textContainerInset.top, attributes: attributes)
            context?.restoreGState()
            return
        }
        guard let index = textView.lineIndex,
              index.utf16Count == textView.textStorage.length else {
            context?.restoreGState()
            return
        }

        let glyphRange = textView.visibleGlyphRange()
        textView.layoutManager.enumerateLineFragments(forGlyphRange: glyphRange) {
            [weak self] _, usedRect, _, fragmentGlyphRange, _ in
            guard let self, let textView = self.textView else { return }
            let characterRange = textView.layoutManager.characterRange(
                forGlyphRange: fragmentGlyphRange,
                actualGlyphRange: nil
            )
            guard let line = index.lineNumber(startingAt: characterRange.location) else {
                // A soft-wrapped continuation deliberately has no number.
                return
            }
            let y = usedRect.minY + textView.textContainerInset.top - textView.contentOffset.y
                + max(0, (usedRect.height - numberFont.lineHeight) * 0.5)
            if y > -20, y < self.bounds.height + 20 {
                self.drawLineNumber(line, y: y, attributes: attributes)
            }
        }

        let visibleCharacters = textView.layoutManager.characterRange(
            forGlyphRange: glyphRange,
            actualGlyphRange: nil
        )
        if index.lines.last?.start == index.utf16Count,
           NSMaxRange(visibleCharacters) >= index.utf16Count {
            let fragment = textView.layoutManager.extraLineFragmentRect
            let y = fragment.minY + textView.textContainerInset.top - textView.contentOffset.y
            if fragment != .zero, y > -20, y < bounds.height + 20 {
                drawLineNumber(index.lines.count, y: y, attributes: attributes)
            }
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
        label.draw(at: CGPoint(x: bounds.maxX - size.width - 8, y: y), withAttributes: attributes)
    }
}
