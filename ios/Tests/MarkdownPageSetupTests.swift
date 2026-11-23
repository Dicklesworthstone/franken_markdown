import Foundation
import XCTest
@testable import FrankenMarkdown

final class MarkdownPageSetupTests: XCTestCase {
    func testDefaultKeepsLegacyExportPath() throws {
        let page = MarkdownPageSetup.standard
        let geometry = try page.geometry()
        XCTAssertEqual(geometry.widthPt, 612)
        XCTAssertEqual(geometry.heightPt, 792)
        XCTAssertEqual(geometry.contentWidthPt, 468)
        XCTAssertEqual(geometry.contentHeightPt, 648)
        XCTAssertNil(try page.bridgePage())
    }

    func testPresetsResolveForTheBundledRendererAndOrientationKeepsMarginSides() throws {
        let presets: [(MarkdownPageSetup.Paper, Double, Double)] = [
            (.letter, 612, 792), (.a4, 595.2755905511812, 841.8897637795276),
            (.a5, 419.52755905511816, 595.2755905511812),
            (.legal, 612, 1_008), (.tabloid, 792, 1_224)
        ]
        for (paper, width, height) in presets {
            var page = MarkdownPageSetup(paper: paper)
            page.margins = .init(topPt: 36, rightPt: 48, bottomPt: 60, leftPt: 84)
            for orientation in MarkdownPageSetup.Orientation.allCases {
                page.orientation = orientation
                let geometry = try page.geometry()
                XCTAssertEqual(geometry.widthPt, orientation == .portrait ? width : height, accuracy: 0.000_001)
                XCTAssertEqual(geometry.heightPt, orientation == .portrait ? height : width, accuracy: 0.000_001)
                let bridge = try XCTUnwrap(page.bridgePage())
                XCTAssertTrue(JSONSerialization.isValidJSONObject(bridge))
                XCTAssertEqual(bridge["size"] as? [String: Double], [
                    "widthPt": geometry.widthPt, "heightPt": geometry.heightPt
                ])
                XCTAssertEqual(bridge["margins"] as? [String: Double], [
                    "topPt": 36, "rightPt": 48, "bottomPt": 60, "leftPt": 84
                ])
                XCTAssertNil(bridge["orientation"])
            }
        }
    }

    func testCustomDimensionsRoundTripWithoutRoundingOrRotatingMargins() throws {
        let page = MarkdownPageSetup(
            paper: .custom, orientation: .landscape,
            customWidthPt: 450.125, customHeightPt: 620.375,
            margins: .init(topPt: 20.5, rightPt: 31.25, bottomPt: 42.75, leftPt: 53.125)
        )
        let recovered = try JSONDecoder().decode(MarkdownPageSetup.self, from: JSONEncoder().encode(page))
        XCTAssertEqual(recovered, page)
        let geometry = try recovered.geometry()
        XCTAssertEqual(geometry.widthPt, 620.375)
        XCTAssertEqual(geometry.heightPt, 450.125)
        XCTAssertEqual(geometry.contentWidthPt, 536)
        XCTAssertEqual(geometry.contentHeightPt, 386.875)
        XCTAssertEqual(geometry.margins, page.margins)
    }

    func testFiniteRangesAndPrintableAreaAreValidatedBeforeExport() throws {
        for bad in [Double.nan, .infinity, -.infinity, 143.999, 14_400.001] {
            var page = MarkdownPageSetup(paper: .custom)
            page.customWidthPt = bad
            XCTAssertThrowsError(try page.bridgePage())
            page.customWidthPt = 612
            page.customHeightPt = bad
            XCTAssertThrowsError(try page.bridgePage())
        }
        for bad in [Double.nan, .infinity, -.infinity, -0.001, 14_400.001] {
            var page = MarkdownPageSetup.standard
            page.margins.leftPt = bad
            XCTAssertThrowsError(try page.bridgePage())
        }
        var page = MarkdownPageSetup.standard
        page.margins = .init(topPt: 360, rightPt: 270, bottomPt: 360, leftPt: 270)
        XCTAssertEqual(try page.geometry().contentWidthPt, 72)
        XCTAssertEqual(try page.geometry().contentHeightPt, 72)
        page.margins.rightPt += 0.000_001
        XCTAssertThrowsError(try page.bridgePage()) { error in
            XCTAssertEqual(error as? MarkdownPageSetup.ValidationError, .contentWidth)
        }
        page.margins.rightPt = 270
        page.margins.bottomPt += 0.000_001
        XCTAssertThrowsError(try page.bridgePage()) { error in
            XCTAssertEqual(error as? MarkdownPageSetup.ValidationError, .contentHeight)
        }
    }

    func testNonfiniteDormantCustomDimensionsCannotBreakNamedPaperRecovery() throws {
        for value in [Double.nan, .infinity, -.infinity] {
            var page = MarkdownPageSetup(paper: .a4, orientation: .landscape)
            page.customWidthPt = value
            page.customHeightPt = value
            page.margins.leftPt = 36
            XCTAssertNoThrow(try page.bridgePage(), "Named paper owns its dimensions")
            let snapshot = try XCTUnwrap(page.recoverySnapshot)
            let restored = try JSONDecoder().decode(MarkdownPageSetup.self, from: JSONEncoder().encode(snapshot))
            XCTAssertEqual(restored.paper, .a4)
            XCTAssertEqual(restored.orientation, .landscape)
            XCTAssertEqual(try restored.geometry(), try page.geometry())
            XCTAssertTrue(restored.customWidthPt.isFinite)
            XCTAssertTrue(restored.customHeightPt.isFinite)
            page.paper = .custom
            XCTAssertNil(page.recoverySnapshot, "An invalid active custom sheet must retain the prior valid recovery setup")
        }
    }

    func testFloatRoundingCannotShrinkAcceptedContentBelowNativeMinimum() {
        let page = MarkdownPageSetup(
            paper: .custom, customWidthPt: 816.4909835206345, customHeightPt: 1166.8058596551418,
            margins: .init(topPt: 72, rightPt: 347.5558721843905, bottomPt: 72, leftPt: 396.93511133624395)
        )
        XCTAssertEqual(page.customWidthPt - page.margins.leftPt - page.margins.rightPt, 72)
        XCTAssertThrowsError(try page.bridgePage()) { error in
            XCTAssertEqual(error as? MarkdownPageSetup.ValidationError, .contentWidth)
        }
    }

    func testMinimumAndMaximumSheetsAreAcceptedWithUsableMargins() throws {
        for size in [144.0, 14_400] {
            let page = MarkdownPageSetup(
                paper: .custom, customWidthPt: size, customHeightPt: size,
                margins: .init(topPt: 0, rightPt: 0, bottomPt: 0, leftPt: 0)
            )
            XCTAssertEqual(try page.geometry().contentWidthPt, size)
            XCTAssertEqual(try page.geometry().contentHeightPt, size)
        }
    }

    @MainActor
    func testInvalidLivePageDoesNotPreventRecoveringSourceWithLastValidPage() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        let store = MarkdownDraftStore(fileURL: directory.appendingPathComponent("draft.json"))
        let model = MarkdownRendererModel(draftStore: store)
        let recoveryDeadline = ContinuousClock.now.advanced(by: .seconds(5))
        while !model.draftRecoveryIsComplete, ContinuousClock.now < recoveryDeadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertTrue(model.draftRecoveryIsComplete)
        let valid = MarkdownPageSetup(paper: .a4, orientation: .landscape)
        model.pdfPageSetup = valid
        model.pdfPageSetup.margins.leftPt = 14_400
        model.source = "# Keep these edits\n\nInvalid page geometry must not disable recovery."
        model.scheduleDraftSave()
        let saveDeadline = ContinuousClock.now.advanced(by: .seconds(5))
        while model.draftStatus != "Saved locally", ContinuousClock.now < saveDeadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertEqual(model.draftStatus, "Saved locally")
        let restored = await store.load()
        XCTAssertEqual(restored?.source, model.source)
        XCTAssertEqual(restored?.pdfPageSetup, valid)
        XCTAssertNotNil(model.pdfPageSetup.validationMessage)
        do {
            _ = try await model.exportPdf()
            XCTFail("Invalid live geometry must not export the recovery fallback")
        } catch {
            XCTAssertEqual(error as? MarkdownPageSetup.ValidationError, .contentWidth)
        }
    }
}
