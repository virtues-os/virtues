// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-eventkit",
    platforms: [
        .macOS(.v10_13),
        .iOS(.v14),
    ],
    products: [
        .library(
            name: "tauri-plugin-eventkit",
            type: .static,
            targets: ["tauri-plugin-eventkit"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-eventkit",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)
