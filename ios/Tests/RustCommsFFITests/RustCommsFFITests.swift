import XCTest
import BingleTestFFI

final class RustCommsFFITests: XCTestCase {
    func test_canImportModule() {
        // Verify the module is available to the test target
        XCTAssertTrue(true)
    }

    func test_algo_ops_suite() {
        XCTAssertEqual(rust_comms_run_algo_ops_tests(), UInt8(1))
    }

    func test_algo_ops_more_suite() {
        XCTAssertEqual(rust_comms_run_algo_ops_more_tests(), UInt8(1))
    }

    func test_asset_ops_suite() {
        XCTAssertEqual(rust_comms_run_asset_ops_tests(), UInt8(1))
    }
    
    func test_stun_suite() {
        XCTAssertEqual(rust_comms_run_stun_tests(), UInt8(1))
    }
}
