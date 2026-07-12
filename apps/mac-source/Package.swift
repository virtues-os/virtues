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
        // The iroh client FFI (uniffi over the Rust `virtues-iroh-ffi` crate),
        // built for macOS by `crates/virtues-iroh-ffi/build-macos.sh`. Lets the
        // collector reach the box over iroh (dial-by-EndpointId), so ingest is
        // authenticated by this device's allowlisted key — no bearer.
        .binaryTarget(
            name: "VirtuesIrohMac",
            path: "../../crates/virtues-iroh-ffi/generated/VirtuesIrohMac.xcframework"
        ),
        .executableTarget(
            name: "VirtuesCollector",
            dependencies: [
                .product(name: "ArgumentParser", package: "swift-argument-parser"),
                "VirtuesIrohMac"
            ],
            path: "Sources",
            linkerSettings: [
                .linkedLibrary("sqlite3"),
                .linkedFramework("Security"),
                .linkedFramework("IOKit"),
                // Required by the iroh/quinn networking stack in VirtuesIrohMac.
                .linkedFramework("SystemConfiguration"),
                // iroh's macOS network monitor uses CoreWLAN (`CWWiFiClient`) to
                // watch WiFi changes; without this the Obj-C class isn't found at
                // runtime and every upload panics ("class CWWiFiClient could not be
                // found").
                .linkedFramework("CoreWLAN")
            ]
        )
    ]
)
