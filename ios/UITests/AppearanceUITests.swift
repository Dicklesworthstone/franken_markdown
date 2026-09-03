import XCTest

final class FrankenMarkdownAppearanceUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testAppearanceTogglePersistsLightModeAcrossLaunches() throws {
        let app = XCUIApplication()
        app.launch()

        let toggle = app.buttons["appearance-toggle"]
        XCTAssertTrue(toggle.waitForExistence(timeout: 12))
        XCTAssertTrue(
            ["Switch to light mode", "Switch to dark mode"].contains(toggle.label),
            "Appearance control exposed an unexpected state: \(toggle.label)"
        )

        if toggle.label == "Switch to dark mode" {
            toggle.tap()
            XCTAssertEqual(toggle.label, "Switch to light mode")
        }

        toggle.tap()
        XCTAssertEqual(toggle.label, "Switch to dark mode")
        keepScreenshot(of: app, named: "Remembered light appearance")

        app.terminate()
        app.launch()

        let relaunchedToggle = app.buttons["appearance-toggle"]
        XCTAssertTrue(relaunchedToggle.waitForExistence(timeout: 12))
        XCTAssertEqual(relaunchedToggle.label, "Switch to dark mode")
    }

    func testEditorialStudioExposesCorePDFTypographyControls() throws {
        let app = XCUIApplication()
        app.launchEnvironment["FMD_OPEN_DOCUMENT_LAB"] = "1"
        app.launch()

        let editorialStudio = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "Editorial Studio")
        ).firstMatch
        XCTAssertTrue(editorialStudio.waitForExistence(timeout: 12))
        editorialStudio.tap()

        XCTAssertTrue(app.buttons["custom-css-import"].waitForExistence(timeout: 5))
        let customTypography = app.switches["custom-pdf-typography-toggle"]
        XCTAssertTrue(customTypography.waitForExistence(timeout: 5))
        if customTypography.value as? String != "1" { customTypography.tap() }

        XCTAssertTrue(app.sliders["Body"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.sliders["Heading ratio"].exists)
        XCTAssertTrue(app.sliders["Tables"].exists)
    }

    private func keepScreenshot(of app: XCUIApplication, named name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
