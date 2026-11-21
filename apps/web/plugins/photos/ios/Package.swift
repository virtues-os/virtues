// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-photos",
    platforms: [
        .macOS("13.0"),
        .iOS("16.0"),
    ],
    products: [
        .library(
            name: "tauri-plugin-photos",
            type: .static,
            targets: ["tauri-plugin-photos"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-photos",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)
