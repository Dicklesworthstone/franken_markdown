import CryptoKit
import Foundation

enum MarkdownResourceKind: String, Codable, Sendable {
    case image
    case font
}

enum MarkdownFontSlot: String, CaseIterable, Identifiable, Sendable {
    case bodyRegular = "body-regular"
    case bodyBold = "body-bold"
    case bodyItalic = "body-italic"
    case bodyBoldItalic = "body-bold-italic"
    case monoRegular = "mono-regular"

    var id: Self { self }
    var title: String {
        switch self {
        case .bodyRegular: "Body regular"
        case .bodyBold: "Body bold"
        case .bodyItalic: "Body italic"
        case .bodyBoldItalic: "Body bold italic"
        case .monoRegular: "Code / monospace"
        }
    }
}

/// Selected bytes are immutable snapshots. A Markdown destination is an exact
/// UTF-8 key, not a filesystem path or permission to fetch a URL.
struct MarkdownResourceAttachment: Identifiable, Equatable, Sendable {
    let kind: MarkdownResourceKind
    let key: String
    let filename: String
    let bytes: Data

    // Swift's String equality merges canonically equivalent spellings. The
    // renderer's destination keys do not, so use an ASCII identity of the bytes.
    var id: String { kind.rawValue + ":" + Data(key.utf8).base64EncodedString() }

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.kind == rhs.kind && lhs.key.utf8.elementsEqual(rhs.key.utf8)
            && lhs.filename.utf8.elementsEqual(rhs.filename.utf8) && lhs.bytes == rhs.bytes
    }
}

struct MarkdownResourceCollection: Equatable, Sendable {
    static let maximumAssetBytes = 16 * 1_024 * 1_024
    static let maximumTotalBytes = 32 * 1_024 * 1_024
    static let maximumImages = 128
    static let maximumKeyBytes = 4_096
    static let empty = Self(entries: [])

    private(set) var entries: [MarkdownResourceAttachment]
    var byteCount: Int { entries.reduce(0) { $0 + $1.bytes.count } }

    enum ResourceError: Error, LocalizedError, Equatable {
        case invalidKey
        case invalidFile
        case unsupportedFormat
        case oversizedAsset
        case oversizedCollection
        case duplicateKey
        case invalidSnapshot
        case importInProgress

        var errorDescription: String? {
            switch self {
            case .invalidKey: "Enter an image destination from your Markdown, or choose one of the five font slots."
            case .invalidFile: "Choose a regular, nonempty image or font file."
            case .unsupportedFormat: "Use PNG, JPEG, GIF or SVG images, or a TrueType/OpenType font."
            case .oversizedAsset: "Each attachment must be no larger than 16 MiB."
            case .oversizedCollection: "A document can contain up to 128 image attachments and 32 MiB of attached bytes."
            case .duplicateKey: "Two attachments cannot use the same image destination or font slot."
            case .invalidSnapshot: "The saved attachment snapshot is missing or has changed. Reattach the affected files before publishing."
            case .importInProgress: "Wait for the selected attachment to finish copying before publishing."
            }
        }
    }

    init(validating entries: [MarkdownResourceAttachment]) throws {
        var identities = Set<String>()
        var total = 0
        var images = 0
        for entry in entries {
            try Self.validateKey(entry.key, kind: entry.kind)
            guard !entry.filename.isEmpty, entry.filename.utf8.count <= 1_024,
                  !entry.bytes.isEmpty else { throw ResourceError.invalidFile }
            guard entry.bytes.count <= Self.maximumAssetBytes else { throw ResourceError.oversizedAsset }
            guard identities.insert(entry.id).inserted else { throw ResourceError.duplicateKey }
            total += entry.bytes.count
            if entry.kind == .image { images += 1 }
            guard total <= Self.maximumTotalBytes, images <= Self.maximumImages else {
                throw ResourceError.oversizedCollection
            }
        }
        self.entries = entries
    }

    private init(entries: [MarkdownResourceAttachment]) { self.entries = entries }

    static func validateKey(_ key: String, kind: MarkdownResourceKind) throws {
        guard !key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              key.utf8.count <= maximumKeyBytes,
              !key.unicodeScalars.contains(where: { $0.value == 0 || $0.value == 10 || $0.value == 13 }),
              kind != .font || MarkdownFontSlot(rawValue: key) != nil else {
            throw ResourceError.invalidKey
        }
    }

    func replacing(_ attachment: MarkdownResourceAttachment) throws -> Self {
        var next = entries
        if let index = next.firstIndex(where: { $0.id == attachment.id }) {
            next[index] = attachment
        } else {
            next.append(attachment)
        }
        return try Self(validating: next)
    }

    func removing(id: String) -> Self {
        Self(entries: entries.filter { $0.id != id })
    }
}

/// Resource files stay beside their scene's draft, outside its small JSON
/// manifest. Only a complete, integrity-checked snapshot is made recoverable.
actor MarkdownResourceStore {
    private struct Entry: Codable {
        let kind: MarkdownResourceKind
        let key: String
        let filename: String
        let byteCount: Int
        let sha256: Data
    }

    private struct Manifest: Codable {
        let schema: Int
        let entries: [Entry]
    }

    private let directory: URL
    private static let maximumManifestBytes = 4 * 1_024 * 1_024

    init(directory: URL) { self.directory = directory }

    func selectedFile(
        at url: URL, kind: MarkdownResourceKind, key: String
    ) throws -> MarkdownResourceAttachment {
        try Task.checkCancellation()
        guard url.isFileURL else { throw MarkdownResourceCollection.ResourceError.invalidFile }
        let destination = kind == .image && key.isEmpty ? url.lastPathComponent : key
        try MarkdownResourceCollection.validateKey(destination, kind: kind)
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<Data, Error>?
        coordinator.coordinate(readingItemAt: url, options: [], error: &coordinationError) { location in
            result = Result { try Self.readFile(location, maximumBytes: MarkdownResourceCollection.maximumAssetBytes) }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw MarkdownResourceCollection.ResourceError.invalidFile }
        let bytes = try result.get()
        guard Self.supports(bytes, kind: kind) else {
            throw MarkdownResourceCollection.ResourceError.unsupportedFormat
        }
        let attachment = MarkdownResourceAttachment(
            kind: kind, key: destination, filename: url.lastPathComponent, bytes: bytes
        )
        _ = try MarkdownResourceCollection(validating: [attachment])
        return attachment
    }

    func save(_ resources: MarkdownResourceCollection) throws -> UUID {
        try Task.checkCancellation()
        _ = try MarkdownResourceCollection(validating: resources.entries)
        try Self.prepareDirectory(directory)
        let identity = UUID()
        let target = directory.appendingPathComponent(identity.uuidString, isDirectory: true)
        try Self.prepareDirectory(target)
        var published = false
        defer { if !published { try? FileManager.default.removeItem(at: target) } }
        var entries: [Entry] = []
        for (index, attachment) in resources.entries.enumerated() {
            try Task.checkCancellation()
            let file = target.appendingPathComponent("\(index).bin", isDirectory: false)
            try attachment.bytes.write(to: file, options: [.withoutOverwriting, .completeFileProtection])
            entries.append(Entry(
                kind: attachment.kind, key: attachment.key, filename: attachment.filename,
                byteCount: attachment.bytes.count, sha256: Data(SHA256.hash(data: attachment.bytes))
            ))
        }
        let bytes = try JSONEncoder().encode(Manifest(schema: 1, entries: entries))
        guard bytes.count <= Self.maximumManifestBytes else {
            throw MarkdownResourceCollection.ResourceError.invalidSnapshot
        }
        try Task.checkCancellation()
        try bytes.write(to: target.appendingPathComponent("manifest.json"), options: [.atomic, .completeFileProtection])
        published = true
        return identity
    }

    func load(_ identity: UUID) throws -> MarkdownResourceCollection {
        try Task.checkCancellation()
        try Self.checkDirectory(directory)
        let target = directory.appendingPathComponent(identity.uuidString, isDirectory: true)
        try Self.checkDirectory(target)
        let bytes = try Self.readFile(
            target.appendingPathComponent("manifest.json"), maximumBytes: Self.maximumManifestBytes
        )
        let manifest = try JSONDecoder().decode(Manifest.self, from: bytes)
        guard manifest.schema == 1,
              manifest.entries.count <= MarkdownResourceCollection.maximumImages + MarkdownFontSlot.allCases.count else {
            throw MarkdownResourceCollection.ResourceError.invalidSnapshot
        }
        // Admit the complete budget before reading any attachment data.
        var total = 0
        for entry in manifest.entries {
            guard entry.byteCount > 0, entry.byteCount <= MarkdownResourceCollection.maximumAssetBytes,
                  entry.sha256.count == 32 else { throw MarkdownResourceCollection.ResourceError.invalidSnapshot }
            total += entry.byteCount
            guard total <= MarkdownResourceCollection.maximumTotalBytes else {
                throw MarkdownResourceCollection.ResourceError.oversizedCollection
            }
        }
        var attachments: [MarkdownResourceAttachment] = []
        for (index, entry) in manifest.entries.enumerated() {
            try Task.checkCancellation()
            let bytes = try Self.readFile(
                target.appendingPathComponent("\(index).bin"), maximumBytes: entry.byteCount
            )
            guard bytes.count == entry.byteCount, Data(SHA256.hash(data: bytes)) == entry.sha256 else {
                throw MarkdownResourceCollection.ResourceError.invalidSnapshot
            }
            attachments.append(MarkdownResourceAttachment(
                kind: entry.kind, key: entry.key, filename: entry.filename, bytes: bytes
            ))
        }
        return try MarkdownResourceCollection(validating: attachments)
    }

    /// Only superseded identities explicitly owned by the editor are cleaned.
    /// Scanning the directory could delete a newer snapshot awaiting publication.
    func discard(_ identities: Set<UUID>, keeping current: UUID?) {
        guard (try? Self.checkDirectory(directory)) != nil else { return }
        for identity in identities where identity != current {
            let target = directory.appendingPathComponent(identity.uuidString, isDirectory: true)
            guard (try? Self.checkDirectory(target)) != nil else { continue }
            try? FileManager.default.removeItem(at: target)
        }
    }

    private static func prepareDirectory(_ url: URL) throws {
        try FileManager.default.createDirectory(
            at: url, withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication]
        )
        try checkDirectory(url)
        var protected = url
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try protected.setResourceValues(values)
    }

    private static func checkDirectory(_ url: URL) throws {
        let values = try url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink == false else {
            throw MarkdownResourceCollection.ResourceError.invalidSnapshot
        }
    }

    private static func readFile(_ url: URL, maximumBytes: Int) throws -> Data {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink == false,
              let size = values.fileSize, size > 0 else {
            throw MarkdownResourceCollection.ResourceError.invalidFile
        }
        guard size <= maximumBytes else { throw MarkdownResourceCollection.ResourceError.oversizedAsset }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var bytes = Data()
        bytes.reserveCapacity(size)
        while true {
            try Task.checkCancellation()
            let remaining = maximumBytes - bytes.count
            let chunk = try handle.read(upToCount: min(64 * 1_024, remaining + 1)) ?? Data()
            if chunk.isEmpty { break }
            guard chunk.count <= remaining else { throw MarkdownResourceCollection.ResourceError.oversizedAsset }
            bytes.append(chunk)
        }
        guard !bytes.isEmpty else { throw MarkdownResourceCollection.ResourceError.invalidFile }
        return bytes
    }

    private static func supports(_ bytes: Data, kind: MarkdownResourceKind) -> Bool {
        let prefix = Array(bytes.prefix(12))
        if kind == .font {
            return prefix.starts(with: [0, 1, 0, 0]) || prefix.starts(with: Array("OTTO".utf8))
                || prefix.starts(with: Array("true".utf8))
        }
        if prefix.starts(with: [137, 80, 78, 71, 13, 10, 26, 10])
            || prefix.starts(with: [255, 216, 255])
            || prefix.starts(with: Array("GIF87a".utf8))
            || prefix.starts(with: Array("GIF89a".utf8)) { return true }
        // This is format admission only. The renderer independently parses and
        // sanitizes SVG, and never receives authority to load its references.
        return String(decoding: bytes.prefix(4_096), as: UTF8.self).contains("<svg")
    }
}
