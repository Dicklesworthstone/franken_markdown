import Darwin
import Foundation
import SwiftUI

/// Chapter paths are renderer keys, not additional filesystem authority.
/// Swift String equality folds canonical Unicode spellings; the renderer does
/// not, so row and collision identities retain the exact UTF-8 bytes.
struct BookSourceFile: Identifiable, Hashable, Sendable {
    let path: String
    let source: String
    var id: Data { Data(path.utf8) }

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.path.utf8.elementsEqual(rhs.path.utf8) &&
            lhs.source.utf8.elementsEqual(rhs.source.utf8)
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
        hasher.combine(source.utf8.count)
        for byte in source.utf8 { hasher.combine(byte) }
    }
}

/// Capture an entire selected collection before publishing any of it to the
/// editor. Bounds match the bundled book API; traversal also has its own bound.
enum MarkdownBookImporter {
    struct Limits: Sendable {
        var chapters = 4_096
        var bytes = 64 * 1_024 * 1_024
        var entries = 65_536
        static let standard = Self()
    }

    enum ImportError: Error, LocalizedError, Equatable {
        case noMarkdownFiles
        case invalidSelection
        case tooManyFiles
        case tooManyEntries
        case tooLarge
        case invalidPath(String)
        case duplicatePath(String)
        case outputCollision(String, String)
        case symbolicLink(String)
        case notRegularFile(String)
        case invalidUTF8(String)
        case movedOutsideSelection(String)

        var errorDescription: String? {
            switch self {
            case .noMarkdownFiles:
                "No .md or .markdown files were found. Your previous collection is still available."
            case .invalidSelection:
                "Choose Markdown files or a folder from Files."
            case .tooManyFiles:
                "Choose at most 4,096 Markdown chapters."
            case .tooManyEntries:
                "That folder contains too many entries to inspect. Choose a smaller document folder."
            case .tooLarge:
                "Collection source files and chapter paths must fit within 64 MiB. Choose a smaller collection."
            case .invalidPath(let path):
                "Rename \(path): use .md or .markdown, without colons, backslashes, control characters or surrounding whitespace."
            case .duplicatePath(let path):
                "More than one selected file has the chapter path \(path). Choose one root folder or rename a chapter."
            case .outputCollision(let first, let second):
                "\(first) and \(second) produce the same publication filename. Rename one chapter before importing."
            case .symbolicLink(let path):
                "\(path) is a symbolic link. Collection imports read regular files inside the selected folder; copy the source there or select its actual file."
            case .notRegularFile(let path):
                "\(path) is not a regular Markdown file."
            case .invalidUTF8(let path):
                "\(path) is not valid UTF-8 text. Save it as UTF-8 before importing."
            case .movedOutsideSelection(let path):
                "\(path) moved outside the selected folder. Choose the collection again."
            }
        }
    }

    private struct Candidate {
        let path: String
        let components: [String]
    }

    private struct Selection {
        let url: URL
        let directory: Bool
        let candidates: [Candidate]
    }

    static func load(from urls: [URL]) async throws -> [BookSourceFile] {
        try Task.checkCancellation()
        let worker = Task.detached(priority: .userInitiated) {
            try readSelection(from: urls)
        }
        return try await withTaskCancellationHandler {
            let files = try await worker.value
            try Task.checkCancellation()
            return files
        } onCancel: {
            worker.cancel()
        }
    }

    static func readSelection(
        from urls: [URL], limits: Limits = .standard
    ) throws -> [BookSourceFile] {
        try Task.checkCancellation()
        guard limits.chapters > 0, limits.bytes >= 0, limits.entries > 0 else {
            throw ImportError.invalidSelection
        }
        guard !urls.isEmpty else { throw ImportError.noMarkdownFiles }
        guard urls.count <= limits.chapters else { throw ImportError.tooManyFiles }
        var selections: [Selection] = []
        var paths = Set<Data>()
        var outputs: [Data: String] = [:]
        var pathBytes = 0
        var visited = 0

        func admit(_ path: String, components: [String]) throws -> Candidate {
            let canonical = try canonicalPath(path)
            guard paths.count < limits.chapters else { throw ImportError.tooManyFiles }
            let key = Data(canonical.utf8)
            guard paths.insert(key).inserted else { throw ImportError.duplicatePath(canonical) }
            let output = outputKey(canonical)
            if let previous = outputs[output] {
                let pair = [previous, canonical].sorted { $0.utf8.lexicographicallyPrecedes($1.utf8) }
                throw ImportError.outputCollision(pair[0], pair[1])
            }
            outputs[output] = canonical
            guard key.count <= limits.bytes - pathBytes else { throw ImportError.tooLarge }
            pathBytes += key.count
            return Candidate(path: canonical, components: components)
        }

        // Discover and validate every name before reading any source. Failed
        // enumeration is an error, never a partial "complete" collection.
        for url in urls {
            try Task.checkCancellation()
            let selection = try coordinated(url) { root, _ -> Selection in
                let type = try itemType(at: root)
                if type == .typeSymbolicLink { throw ImportError.symbolicLink(url.lastPathComponent) }
                if type == .typeDirectory {
                    var enumerationError: Error?
                    guard let enumerator = FileManager.default.enumerator(
                        at: root, includingPropertiesForKeys: nil,
                        options: [.skipsHiddenFiles, .skipsPackageDescendants],
                        errorHandler: { _, error in enumerationError = error; return false }
                    ) else { throw ImportError.invalidSelection }
                    var candidates: [Candidate] = []
                    for case let child as URL in enumerator {
                        try Task.checkCancellation()
                        guard visited < limits.entries else { throw ImportError.tooManyEntries }
                        visited += 1
                        let components = try relativeComponents(of: child, in: root)
                        let path = components.joined(separator: "/")
                        let type = try itemType(at: child)
                        if type == .typeSymbolicLink {
                            enumerator.skipDescendants()
                            throw ImportError.symbolicLink(path)
                        }
                        if type == .typeDirectory { continue }
                        guard isMarkdown(child) else { continue }
                        guard type == .typeRegular else { throw ImportError.notRegularFile(path) }
                        candidates.append(try admit(path, components: components))
                    }
                    if let enumerationError { throw enumerationError }
                    return Selection(url: url, directory: true, candidates: candidates)
                }
                guard type == .typeRegular else { throw ImportError.notRegularFile(url.lastPathComponent) }
                guard isMarkdown(url) else { return Selection(url: url, directory: false, candidates: []) }
                let candidate = try admit(url.lastPathComponent, components: [])
                return Selection(url: url, directory: false, candidates: [candidate])
            }
            selections.append(selection)
        }
        guard !paths.isEmpty else { throw ImportError.noMarkdownFiles }

        var files: [BookSourceFile] = []
        files.reserveCapacity(paths.count)
        var consumed = pathBytes
        for selection in selections where !selection.candidates.isEmpty {
            try Task.checkCancellation()
            if selection.directory {
                // Prepare the selected top-level directory only. Foundation
                // notifies its contained presenters for the later child reads.
                try coordinatedBatch(at: selection.url) { coordinator in
                    // Root and chapter accessors are siblings, not recursively
                    // coordinated reads inside a held directory accessor.
                    let (root, directory) = try coordinateRead(
                        selection.url, coordinator: coordinator, options: .withoutChanges
                    ) { location in
                        (location, try openDirectory(at: location))
                    }
                    defer { _ = Darwin.close(directory) }
                    for candidate in selection.candidates {
                        try Task.checkCancellation()
                        let child = candidate.components.reduce(root) { $0.appendingPathComponent($1) }
                        let loaded = try coordinateRead(
                            child, coordinator: coordinator, options: .withoutChanges
                        ) { location in
                            let actual = try relativeComponents(of: location, in: root)
                            guard componentIdentity(actual) == componentIdentity(candidate.components) else {
                                throw ImportError.movedOutsideSelection(candidate.path)
                            }
                            let handle = try openRelativeFile(candidate.components, in: directory, path: candidate.path)
                            defer { try? handle.close() }
                            return try readRegularFile(handle, path: candidate.path, maximumBytes: limits.bytes - consumed)
                        }
                        consumed += loaded.diskFingerprint.byteCount
                        files.append(BookSourceFile(path: candidate.path, source: loaded.source))
                    }
                }
            } else if let candidate = selection.candidates.first {
                let loaded = try coordinated(selection.url) { root, _ in
                    let handle = try openFile(at: root, path: candidate.path)
                    defer { try? handle.close() }
                    return try readRegularFile(handle, path: candidate.path, maximumBytes: limits.bytes - consumed)
                }
                consumed += loaded.diskFingerprint.byteCount
                files.append(BookSourceFile(path: candidate.path, source: loaded.source))
            }
        }
        try Task.checkCancellation()
        // Publication order must not depend on directory enumeration or the
        // device's language. Explicit frontmatter ordering remains core-owned.
        return files.sorted { $0.path.utf8.lexicographicallyPrecedes($1.path.utf8) }
    }

    static func canonicalPath(_ path: String) throws -> String {
        // The pinned native package trims input names and treats backslashes as
        // separators. Refuse filenames it cannot preserve instead of silently
        // publishing their content under a different logical name.
        guard !path.isEmpty, path.utf8.count <= 4_096,
              path.utf8.elementsEqual(path.trimmingCharacters(in: .whitespacesAndNewlines).utf8),
              path.unicodeScalars.first?.value != 0xFEFF,
              path.unicodeScalars.last?.value != 0xFEFF,
              !path.hasPrefix("/"), !path.hasSuffix("/"),
              !path.contains("\\"), !path.contains(":"),
              !path.unicodeScalars.contains(where: { $0.value < 0x20 || (0x7F...0x9F).contains($0.value) })
        else { throw ImportError.invalidPath(path) }
        var parts: [Substring] = []
        for part in path.split(separator: "/", omittingEmptySubsequences: true) {
            if part == "." { continue }
            if part == ".." {
                guard !parts.isEmpty else { throw ImportError.invalidPath(path) }
                parts.removeLast()
            } else { parts.append(part) }
        }
        let canonical = parts.joined(separator: "/")
        guard canonical.hasSuffix(".md") || canonical.hasSuffix(".markdown") else {
            throw ImportError.invalidPath(path)
        }
        return canonical
    }

    /// Mirrors the core's portable output-name collision key (ASCII case,
    /// flattened separators, UTF-8 byte escapes), without normalizing Unicode.
    /// Reserved names receive a common prefix in the core and retain this
    /// equivalence relation; this key is never used as an output filename.
    private static func outputKey(_ path: String) -> Data {
        let stem = path.hasSuffix(".markdown") ? path.dropLast(9) : path.dropLast(3)
        let hex = Array("0123456789abcdef".utf8)
        var result = Data()
        for byte in stem.utf8 {
            switch byte {
            case 47: result.append(contentsOf: [95, 95])
            case 65...90: result.append(byte + 32)
            case 97...122, 48...57, 45, 95, 46: result.append(byte)
            default: result.append(contentsOf: [126, hex[Int(byte >> 4)], hex[Int(byte & 15)]])
            }
        }
        return result
    }

    private static func isMarkdown(_ url: URL) -> Bool {
        ["md", "markdown"].contains(url.pathExtension.lowercased())
    }

    private static func componentIdentity(_ parts: [String]) -> [Data] {
        parts.map { Data($0.utf8) }
    }

    private static func relativeComponents(of child: URL, in root: URL) throws -> [String] {
        let base = root.standardizedFileURL.pathComponents
        let parts = child.standardizedFileURL.pathComponents
        guard child.isFileURL, parts.count > base.count,
              componentIdentity(Array(parts.prefix(base.count))) == componentIdentity(base) else {
            throw ImportError.movedOutsideSelection(child.lastPathComponent)
        }
        return Array(parts.dropFirst(base.count))
    }

    private static func itemType(at url: URL) throws -> FileAttributeType {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        guard let type = attributes[.type] as? FileAttributeType else {
            throw ImportError.notRegularFile(url.lastPathComponent)
        }
        return type
    }

    private static func coordinated<T>(
        _ url: URL, operation: (URL, NSFileCoordinator) throws -> T
    ) throws -> T {
        try Task.checkCancellation()
        guard url.isFileURL else { throw ImportError.invalidSelection }
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        let coordinator = NSFileCoordinator()
        return try coordinateRead(url, coordinator: coordinator) {
            try operation($0, coordinator)
        }
    }

    private static func coordinateRead<T>(
        _ url: URL, coordinator: NSFileCoordinator,
        options: NSFileCoordinator.ReadingOptions = [], operation: (URL) throws -> T
    ) throws -> T {
        var coordinationError: NSError?
        var result: Result<T, Error>?
        coordinator.coordinate(readingItemAt: url, options: options, error: &coordinationError) { location in
            result = Result {
                try Task.checkCancellation()
                return try operation(location)
            }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw MarkdownSourceLoader.DocumentError.coordinatedRead }
        return try result.get()
    }

    private static func coordinatedBatch<T>(
        at url: URL, operation: (NSFileCoordinator) throws -> T
    ) throws -> T {
        try Task.checkCancellation()
        guard url.isFileURL else { throw ImportError.invalidSelection }
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<T, Error>?
        coordinator.prepare(
            forReadingItemsAt: [url], options: [],
            writingItemsAt: [], options: [], error: &coordinationError
        ) { finished in
            defer { finished() }
            result = Result {
                try Task.checkCancellation()
                return try operation(coordinator)
            }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw MarkdownSourceLoader.DocumentError.coordinatedRead }
        return try result.get()
    }

    private static func posixFailure(path: String) -> NSError {
        NSError(domain: NSPOSIXErrorDomain, code: Int(errno), userInfo: [NSFilePathErrorKey: path])
    }

    private static func openDirectory(at url: URL) throws -> Int32 {
        let descriptor = Darwin.open(url.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else { throw posixFailure(path: url.path) }
        return descriptor
    }

    private static func openFile(at url: URL, path: String) throws -> FileHandle {
        let descriptor = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { throw posixFailure(path: path) }
        return FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    }

    /// Pin each directory in turn. A replaced symlink cannot escape the root;
    /// opening the final component nonblocking also prevents a swapped FIFO
    /// from hanging before its descriptor type can be checked.
    static func openRelativeFile(
        _ components: [String], in root: Int32, path: String
    ) throws -> FileHandle {
        guard !components.isEmpty, components.allSatisfy({
            !$0.isEmpty && $0 != "." && $0 != ".." && !$0.contains("/") && !$0.contains("\0")
        }) else { throw ImportError.invalidPath(path) }
        var directory = Darwin.dup(root)
        guard directory >= 0 else { throw posixFailure(path: path) }
        defer { _ = Darwin.close(directory) }
        for component in components.dropLast() {
            let next = Darwin.openat(directory, component, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            guard next >= 0 else { throw posixFailure(path: path) }
            _ = Darwin.close(directory)
            directory = next
        }
        let descriptor = Darwin.openat(directory, components[components.count - 1],
                                       O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { throw posixFailure(path: path) }
        return FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    }

    static func readRegularFile(
        _ handle: FileHandle, path: String, maximumBytes: Int
    ) throws -> MarkdownSourceLoader.LoadedSource {
        try Task.checkCancellation()
        var info = stat()
        guard Darwin.fstat(handle.fileDescriptor, &info) == 0 else { throw posixFailure(path: path) }
        guard info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG) else {
            throw ImportError.notRegularFile(path)
        }
        guard maximumBytes >= 0, info.st_size >= 0, info.st_size <= Int64(maximumBytes) else {
            throw ImportError.tooLarge
        }
        do {
            return try MarkdownSourceLoader.readSource(maximumBytes: maximumBytes, capacityHint: Int(info.st_size)) {
                try handle.read(upToCount: $0) ?? Data()
            }
        } catch MarkdownSourceLoader.ImportError.notUTF8 {
            throw ImportError.invalidUTF8(path)
        } catch MarkdownSourceLoader.ImportError.tooLarge {
            throw ImportError.tooLarge
        }
    }
}

/// A picker and its suspended read belong to one document and one request.
/// Completion consults the live document owner before adopting even if SwiftUI
/// has not delivered the corresponding onChange yet.
@MainActor
final class MarkdownBookImportSession: ObservableObject {
    struct Request: Equatable, Sendable {
        let id: UUID
        let documentOwner: UUID
    }

    @Published private(set) var files: [BookSourceFile] = []
    @Published private(set) var isImporting = false
    @Published private(set) var errorMessage: String?
    private var request: Request?
    private var filesOwner: UUID?
    private var task: Task<Void, Never>?
    private let load: @Sendable ([URL]) async throws -> [BookSourceFile]

    init(load: @escaping @Sendable ([URL]) async throws -> [BookSourceFile] = {
        try await MarkdownBookImporter.load(from: $0)
    }) {
        self.load = load
    }

    deinit { task?.cancel() }

    func beginSelection(owner: UUID) -> Request {
        if let filesOwner, filesOwner != owner { resetForDocument() }
        else { cancelPending() }
        let next = Request(id: UUID(), documentOwner: owner)
        request = next
        errorMessage = nil
        return next
    }

    @discardableResult
    func startImport(
        from urls: [URL], request expected: Request,
        currentOwner: @escaping @MainActor () -> UUID
    ) -> Task<Void, Never>? {
        guard request == expected, task == nil else { return nil }
        guard expected.documentOwner == currentOwner() else { resetForDocument(); return nil }
        guard !urls.isEmpty else { cancelPending(); return nil }
        isImporting = true
        let load = self.load
        let work = Task { @MainActor [weak self] in
            let result: Result<[BookSourceFile], Error>
            do {
                try Task.checkCancellation()
                let files = try await load(urls)
                try Task.checkCancellation()
                guard !files.isEmpty else { throw MarkdownBookImporter.ImportError.noMarkdownFiles }
                result = .success(files)
            } catch { result = .failure(error) }
            self?.finish(result, request: expected, currentOwner: currentOwner)
        }
        task = work
        return work
    }

    func selectionFailed(_ error: Error, request expected: Request, currentOwner: UUID) {
        guard request == expected else { return }
        guard expected.documentOwner == currentOwner else { resetForDocument(); return }
        cancelPending()
        let nsError = error as NSError
        if !(error is CancellationError) && !(nsError.domain == NSCocoaErrorDomain && nsError.code == NSUserCancelledError) {
            errorMessage = error.localizedDescription
        }
    }

    func cancelPending() {
        request = nil
        task?.cancel()
        task = nil
        isImporting = false
    }

    func clearFailure() { errorMessage = nil }

    func resetForDocument() {
        cancelPending()
        files = []
        filesOwner = nil
        errorMessage = nil
    }

    func publicationFiles(owner: UUID) -> [BookSourceFile]? {
        guard filesOwner == owner else { resetForDocument(); return nil }
        guard !files.isEmpty, request == nil, !isImporting else { return nil }
        return files
    }

    private func finish(
        _ result: Result<[BookSourceFile], Error>, request expected: Request,
        currentOwner: @MainActor () -> UUID
    ) {
        guard request == expected else { return }
        guard expected.documentOwner == currentOwner() else { resetForDocument(); return }
        request = nil
        task = nil
        isImporting = false
        switch result {
        case .success(let files):
            self.files = files
            filesOwner = expected.documentOwner
            errorMessage = nil
        case .failure(let error):
            if !(error is CancellationError) { errorMessage = error.localizedDescription }
        }
    }
}
