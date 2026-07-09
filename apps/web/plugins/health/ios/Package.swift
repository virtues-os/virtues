// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-health",
    platforms: [
        .macOS(.v10_13),
        .iOS(.v14),
    ],
    products: [
        .library(
            name: "tauri-plugin-health",
            type: .static,
            targets: ["tauri-plugin-health"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-health",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)
