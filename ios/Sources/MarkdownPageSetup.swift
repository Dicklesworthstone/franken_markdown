import Foundation

/// Native editing state for the PDF page contract. Resolve paper names here:
/// the bundled renderer accepts explicit sizes even when a newer preset name
/// is not part of its JavaScript API yet. All measurements are PDF points.
struct MarkdownPageSetup: Codable, Equatable, Sendable {
    enum Paper: String, Codable, CaseIterable, Identifiable, Sendable {
        case letter, a4, a5, legal, tabloid, custom

        var id: Self { self }
        var title: String {
            switch self {
            case .letter: "US Letter"
            case .a4: "A4"
            case .a5: "A5"
            case .legal: "US Legal"
            case .tabloid: "Tabloid"
            case .custom: "Custom"
            }
        }

        fileprivate var dimensions: (width: Double, height: Double)? {
            switch self {
            case .letter: (612, 792)
            case .a4: (210 * 72 / 25.4, 297 * 72 / 25.4)
            case .a5: (148 * 72 / 25.4, 210 * 72 / 25.4)
            case .legal: (612, 1_008)
            case .tabloid: (792, 1_224)
            case .custom: nil
            }
        }
    }

    enum Orientation: String, Codable, CaseIterable, Identifiable, Sendable {
        case portrait, landscape

        var id: Self { self }
        var title: String { self == .portrait ? "Portrait" : "Landscape" }
    }

    struct Margins: Codable, Equatable, Sendable {
        var topPt = 72.0
        var rightPt = 72.0
        var bottomPt = 72.0
        var leftPt = 72.0
    }

    struct Geometry: Equatable, Sendable {
        let widthPt: Double
        let heightPt: Double
        let margins: Margins

        var contentWidthPt: Double { widthPt - margins.leftPt - margins.rightPt }
        var contentHeightPt: Double { heightPt - margins.topPt - margins.bottomPt }
    }

    enum ValidationError: LocalizedError, Equatable {
        case dimension(String)
        case margin(String)
        case contentWidth
        case contentHeight

        var errorDescription: String? {
            switch self {
            case let .dimension(name): "\(name) must be from 144 to 14,400 pt."
            case let .margin(name): "\(name) margin must be from 0 to 14,400 pt."
            case .contentWidth: "Left and right margins must leave at least 72 pt of usable page width."
            case .contentHeight: "Top and bottom margins must leave at least 72 pt of usable page height."
            }
        }
    }

    static let standard = Self()

    var paper: Paper = .letter
    var orientation: Orientation = .portrait
    var customWidthPt = 612.0
    var customHeightPt = 792.0
    var margins = Margins()

    func geometry() throws -> Geometry {
        let dimensions = paper.dimensions ?? (width: customWidthPt, height: customHeightPt)
        for (name, value) in [("Page width", dimensions.width), ("Page height", dimensions.height)] {
            guard value.isFinite, (144...14_400).contains(value) else {
                throw ValidationError.dimension(name)
            }
        }
        for (name, value) in [("Top", margins.topPt), ("Right", margins.rightPt),
                              ("Bottom", margins.bottomPt), ("Left", margins.leftPt)] {
            guard value.isFinite, (0...14_400).contains(value) else {
                throw ValidationError.margin(name)
            }
        }

        // Orientation changes the sheet, not the meaning of each margin.
        let short = min(dimensions.width, dimensions.height)
        let long = max(dimensions.width, dimensions.height)
        let geometry = Geometry(
            widthPt: orientation == .portrait ? short : long,
            heightPt: orientation == .portrait ? long : short,
            margins: margins
        )
        // The WASM ABI uses Float. Check its left-associative arithmetic too,
        // so a rectangle valid in Double cannot shrink below the native limit.
        guard geometry.contentWidthPt >= 72,
              Float(geometry.widthPt) - Float(margins.leftPt) - Float(margins.rightPt) >= 72 else {
            throw ValidationError.contentWidth
        }
        guard geometry.contentHeightPt >= 72,
              Float(geometry.heightPt) - Float(margins.topPt) - Float(margins.bottomPt) >= 72 else {
            throw ValidationError.contentHeight
        }
        return geometry
    }

    var validationMessage: String? {
        do { _ = try geometry(); return nil }
        catch { return error.localizedDescription }
    }

    /// Inactive custom fields do not constrain a named paper export. Keep its
    /// selected geometry recoverable even if those hidden fields cannot be
    /// represented in JSON; active invalid geometry still retains the last
    /// valid recovery snapshot in the renderer model.
    var recoverySnapshot: Self? {
        guard validationMessage == nil else { return nil }
        var snapshot = self
        if !snapshot.customWidthPt.isFinite { snapshot.customWidthPt = Self.standard.customWidthPt }
        if !snapshot.customHeightPt.isFinite { snapshot.customHeightPt = Self.standard.customHeightPt }
        return snapshot
    }

    /// Omitting untouched/default geometry keeps the legacy PDF export path.
    /// Other settings use the canonical shape shared by document and book PDFs.
    func bridgePage() throws -> [String: Any]? {
        let geometry = try geometry()
        guard geometry.widthPt != 612 || geometry.heightPt != 792 || margins != Margins() else {
            return nil
        }
        return [
            "size": ["widthPt": geometry.widthPt, "heightPt": geometry.heightPt],
            "margins": ["topPt": margins.topPt, "rightPt": margins.rightPt,
                        "bottomPt": margins.bottomPt, "leftPt": margins.leftPt]
        ]
    }
}
