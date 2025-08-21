// swift-tools-version:5.7
import PackageDescription

let package = Package(
    name: "RustCommsFFI",
    platforms: [
        .iOS(.v13)
    ],
    products: [
        .library(name: "RustCommsFFI", targets: ["RustCommsFFI"])
    ],
    targets: [
        // Build the XCFramework first via scripts/build_ios_xcframework.sh
        .binaryTarget(name: "RustCommsFFI", path: "RustCommsFFI.xcframework"),
        .testTarget(
            name: "RustCommsFFITests",
            dependencies: ["RustCommsFFI"],
            path: "Tests/RustCommsFFITests"
        )
    ]
)
