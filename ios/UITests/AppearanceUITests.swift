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

        let forgeHome = app.scrollViews["document-forge-home"]
        XCTAssertTrue(forgeHome.waitForExistence(timeout: 12), app.debugDescription)
        let editorialStudio = app.buttons["document-forge-route-settings"]
        for _ in 0..<10 {
            if editorialStudio.exists && editorialStudio.isHittable { break }
            forgeHome.swipeUp()
        }
        XCTAssertTrue(editorialStudio.exists, app.debugDescription)
        XCTAssertTrue(editorialStudio.isHittable, app.debugDescription)
        editorialStudio.tap()

        let studioScroll = app.scrollViews["document-forge-route-scroll-settings"]
        XCTAssertTrue(studioScroll.waitForExistence(timeout: 5), app.debugDescription)
        let cssImport = app.buttons["custom-css-import"]
        for _ in 0..<10 {
            if cssImport.exists && cssImport.isHittable { break }
            studioScroll.swipeUp()
        }
        XCTAssertTrue(cssImport.exists, app.debugDescription)
        XCTAssertTrue(cssImport.isHittable, app.debugDescription)
        keepScreenshot(of: app, named: "Editorial Studio custom CSS controls")

        let customTypography = app.switches["custom-pdf-typography-toggle"]
        for _ in 0..<8 {
            if customTypography.exists && customTypography.isHittable { break }
            studioScroll.swipeUp()
        }
        XCTAssertTrue(customTypography.exists, app.debugDescription)
        XCTAssertTrue(customTypography.isHittable, app.debugDescription)
        if customTypography.value as? String != "1" { customTypography.tap() }

        let bodySize = app.sliders["Body"]
        for _ in 0..<6 {
            if bodySize.exists && bodySize.isHittable { break }
            studioScroll.swipeUp()
        }
        XCTAssertTrue(bodySize.exists, app.debugDescription)
        XCTAssertTrue(app.sliders["Heading ratio"].exists, app.debugDescription)
        XCTAssertTrue(app.sliders["Tables"].exists, app.debugDescription)
        keepScreenshot(of: app, named: "Editorial Studio precise PDF typography")
    }

    private func keepScreenshot(of app: XCUIApplication, named name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
