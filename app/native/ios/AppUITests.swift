import XCTest

final class AppUITests: XCTestCase {
    func testShowsBrandOnLaunch() throws {
        let app = XCUIApplication()
        app.launch()
        let brand = app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] %@", "monochange")).element
        XCTAssertTrue(brand.waitForExistence(timeout: 15))
    }
}
