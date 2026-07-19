// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-audio",
    platforms: [
        .macOS(.v10_13),
        .iOS(.v14),
    ],
    products: [
        .library(
            name: "tauri-plugin-audio",
            type: .static,
            targets: ["tauri-plugin-audio"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-audio",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)
