// swift-tools-version:5.7
import PackageDescription

let package = Package(
    name: "RustCommsFFI",
    platforms: [
        .iOS(.v13)
    ],
    products: [
        // You can depend on either product, or both in your app/test target
        .library(name: "RustCommsFFI", targets: ["RustCommsFFI"]),
        .library(name: "BingleTestFFI", targets: ["BingleTestFFI"]) 
    ],
    targets: [
        .binaryTarget(name: "RustCommsFFI", path: "RustCommsFFI.xcframework"),
        .binaryTarget(name: "BingleTestFFI", path: "BingleTestFFI.xcframework"),
        .testTarget(
                    name: "RustCommsFFITests",
                    dependencies: ["RustCommsFFI","BingleTestFFI"],
                    path: "Tests/RustCommsFFITests"
                )
    ]
)
