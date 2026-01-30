// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "virtues-collector",
    platforms: [
        .macOS(.v12)
    ],
    products: [
        .executable(name: "virtues-collector", targets: ["VirtuesCollector"])
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-argument-parser", from: "1.2.0")
    ],
    targets: [
        .executableTarget(
            name: "VirtuesCollector",
            dependencies: [
                .product(name: "ArgumentParser", package: "swift-argument-parser")
            ],
            path: "Sources",
            linkerSettings: [
                .linkedLibrary("sqlite3"),
                .linkedFramework("Security"),
                .linkedFramework("IOKit")
            ]
        )
    ]
)
