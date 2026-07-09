// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-finance",
    platforms: [
        .macOS(.v10_13),
        .iOS(.v14),
    ],
    products: [
        .library(
            name: "tauri-plugin-finance",
            type: .static,
            targets: ["tauri-plugin-finance"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-finance",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)
