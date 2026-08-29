import XCTest

final class HTMLShareUITests: XCTestCase {
    func testSelfContainedHTMLIsSharedAsAFile() throws {
        let app = XCUIApplication()
        app.launch()

        let publish = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "publish")
        ).firstMatch
        XCTAssertTrue(publish.waitForExistence(timeout: 8))
        publish.tap()

        let html = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "self-contained html")
        ).firstMatch
        XCTAssertTrue(html.waitForExistence(timeout: 3))
        html.tap()

        let activitySheet = app.otherElements["ActivityListView"]
        XCTAssertTrue(
            activitySheet.waitForExistence(timeout: 12),
            "Expected the system activity sheet.\n\(app.debugDescription)"
        )
        let saveToFiles = app.cells.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "save to files")
        ).firstMatch
        XCTAssertTrue(
            saveToFiles.waitForExistence(timeout: 12),
            "Expected an activity sheet with a file destination.\n\(app.debugDescription)"
        )
        saveToFiles.tap()
        let htmlFilename = app.textFields.matching(
            NSPredicate(format: "value ENDSWITH[c] %@", ".html")
        ).firstMatch
        XCTAssertTrue(
            htmlFilename.waitForExistence(timeout: 5),
            "Save to Files did not receive a named HTML file.\n\(app.debugDescription)"
        )
        XCTAssertTrue(app.buttons["Save"].exists)
        app.buttons["Cancel"].tap()
    }
}
