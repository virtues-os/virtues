// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-contacts",
    platforms: [
        .macOS(.v10_13),
        .iOS(.v14),
    ],
    products: [
        .library(
            name: "tauri-plugin-contacts",
            type: .static,
            targets: ["tauri-plugin-contacts"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-contacts",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)
