import Foundation
import SwiftUI

/// The baseline belongs to a document; a comparison additionally belongs to
/// the exact source revision captured at the user's request. Neither identity
/// relies on Swift String equality, which folds distinct Unicode spellings.
@MainActor
final class MarkdownComparisonSession: ObservableObject {
    struct SourceIdentity: Equatable, Sendable {
        let documentOwner: UUID
        let revision: Int
    }

    struct Selection: Equatable, Sendable {
        let id: UUID
        let documentOwner: UUID
    }

    @Published private(set) var baseline: String?
    @Published private(set) var baselineByteCount = 0
    @Published private(set) var isImporting = false
    @Published private(set) var isComparing = false
    @Published private(set) var errorMessage: String?
    @Published private var renderedPreview: SemanticDiffPreview?
    private var documentOwner: UUID?
    private var selection: Selection?
    private var importTask: Task<Void, Never>?
    private var comparison: UUID?
    private var comparisonIdentity: SourceIdentity?
    private var previewIdentity: SourceIdentity?
    private var comparisonTask: Task<Void, Never>?
    private let load: @Sendable (URL) async throws -> String

    init(load: @escaping @Sendable (URL) async throws -> String = {
        try await MarkdownSourceLoader.snapshot(from: $0).source
    }) {
        self.load = load
    }

    deinit {
        importTask?.cancel()
        comparisonTask?.cancel()
    }

    func beginSelection(owner: UUID) -> Selection {
        useDocument(owner)
        cancelPending()
        let request = Selection(id: UUID(), documentOwner: owner)
        selection = request
        isImporting = true
        errorMessage = nil
        return request
    }

    @discardableResult
    func importBaseline(
        from url: URL, selection expected: Selection,
        currentOwner: @escaping @MainActor () -> UUID
    ) -> Task<Void, Never>? {
        guard selection == expected, importTask == nil else { return nil }
        guard expected.documentOwner == currentOwner() else { resetForDocument(); return nil }
        let load = self.load
        let task = Task { @MainActor [weak self] in
            let result: Result<String, Error>
            do {
                try Task.checkCancellation()
                guard expected.documentOwner == currentOwner() else { throw CancellationError() }
                let source = try await load(url)
                try Task.checkCancellation()
                result = .success(source)
            } catch { result = .failure(error) }
            guard let self, self.selection == expected else { return }
            guard expected.documentOwner == currentOwner() else { self.resetForDocument(); return }
            self.selection = nil
            self.importTask = nil
            self.isImporting = false
            switch result {
            case .success(let source): self.adoptBaseline(source)
            case .failure(let error): self.recordFailure(error)
            }
        }
        importTask = task
        return task
    }

    func selectionFailed(_ error: Error, selection expected: Selection, currentOwner: UUID) {
        guard selection == expected else { return }
        guard expected.documentOwner == currentOwner else { resetForDocument(); return }
        cancelImport()
        recordFailure(error)
    }

    func snapshot(_ source: String, owner: UUID) {
        useDocument(owner)
        cancelPending()
        adoptBaseline(source)
    }

    @discardableResult
    func compare(
        source: String, identity: SourceIdentity,
        currentIdentity: @escaping @MainActor () -> SourceIdentity,
        render: @escaping @MainActor (String, String) async throws -> SemanticDiffPreview
    ) -> Task<Void, Never>? {
        guard identity == currentIdentity() else { return nil }
        useDocument(identity.documentOwner)
        guard let baseline, !isImporting else { return nil }
        cancelComparison()
        clearPreview()
        errorMessage = nil
        let request = UUID()
        comparison = request
        comparisonIdentity = identity
        isComparing = true
        let task = Task { @MainActor [weak self] in
            let result: Result<SemanticDiffPreview, Error>
            do {
                try Task.checkCancellation()
                guard identity == currentIdentity() else { throw CancellationError() }
                // Both immutable strings were captured before this Task could
                // yield. The renderer must not reread a newer editor buffer.
                let preview = try await render(baseline, source)
                try Task.checkCancellation()
                result = .success(preview)
            } catch { result = .failure(error) }
            guard let self, self.comparison == request else { return }
            let current = currentIdentity()
            guard identity.documentOwner == current.documentOwner else { self.resetForDocument(); return }
            self.comparison = nil
            self.comparisonIdentity = nil
            self.comparisonTask = nil
            self.isComparing = false
            guard identity == current else { return }
            switch result {
            case .success(let preview):
                self.renderedPreview = preview
                self.previewIdentity = identity
            case .failure(let error): self.recordFailure(error)
            }
        }
        comparisonTask = task
        return task
    }

    /// Check live ownership even before SwiftUI delivers an onChange callback.
    func preview(for identity: SourceIdentity) -> SemanticDiffPreview? {
        guard previewIdentity == identity, documentOwner == identity.documentOwner else { return nil }
        return renderedPreview
    }

    func sourceDidChange(to identity: SourceIdentity) {
        guard documentOwner == identity.documentOwner else { resetForDocument(); return }
        if comparisonIdentity != identity { cancelComparison() }
        if previewIdentity != identity { clearPreview() }
        errorMessage = nil
    }

    func cancelPending() {
        cancelImport()
        cancelComparison()
    }

    func clearFailure() { errorMessage = nil }

    func resetForDocument() {
        cancelPending()
        documentOwner = nil
        baseline = nil
        baselineByteCount = 0
        clearPreview()
        errorMessage = nil
    }

    private func useDocument(_ owner: UUID) {
        if documentOwner != owner { resetForDocument(); documentOwner = owner }
    }

    private func adoptBaseline(_ source: String) {
        let byteCount = source.utf8.count
        guard byteCount <= MarkdownSourceLoader.maximumBytes else {
            recordFailure(MarkdownSourceLoader.ImportError.tooLarge(maximumBytes: MarkdownSourceLoader.maximumBytes))
            return
        }
        baseline = source
        baselineByteCount = byteCount
        clearPreview()
        errorMessage = nil
    }

    private func clearPreview() {
        renderedPreview = nil
        previewIdentity = nil
    }

    private func cancelImport() {
        selection = nil
        importTask?.cancel()
        importTask = nil
        isImporting = false
    }

    private func cancelComparison() {
        comparison = nil
        comparisonIdentity = nil
        comparisonTask?.cancel()
        comparisonTask = nil
        isComparing = false
    }

    private func recordFailure(_ error: Error) {
        // A provider or renderer can ignore cancellation and then throw an
        // unrelated error. Direct Task cancellation must silence that too.
        guard !Task.isCancelled else { return }
        let nsError = error as NSError
        if !(error is CancellationError) && !(nsError.domain == NSCocoaErrorDomain && nsError.code == NSUserCancelledError) {
            errorMessage = error.localizedDescription
        }
    }
}
