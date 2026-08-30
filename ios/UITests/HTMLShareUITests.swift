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
        // The iOS 26 simulator's remote Files activity is not automation-stable: tapping this
        // cell can leave the activity sheet visible without presenting the document browser.
        // Its presence is nevertheless the system-level contract we need here. Plain shared text
        // does not expose the file destination, whereas the temporary `.html` URL does.
        app.buttons["header.closeButton"].tap()
    }
}
