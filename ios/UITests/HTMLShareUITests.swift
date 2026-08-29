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

        let saveToFiles = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "save to files")
        ).firstMatch
        XCTAssertTrue(
            saveToFiles.waitForExistence(timeout: 12),
            "Expected an activity sheet with a file destination.\n\(app.debugDescription)"
        )
        let htmlFilename = app.staticTexts.matching(
            NSPredicate(format: "label ENDSWITH[c] %@", ".html")
        ).firstMatch
        XCTAssertTrue(
            htmlFilename.waitForExistence(timeout: 3),
            "The activity sheet did not identify an HTML file.\n\(app.debugDescription)"
        )
    }
}
