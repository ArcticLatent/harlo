// swift-tools-version:6.0
// HarloKit: the phone protocol (crates/harlo-core/src/phone.rs and
// crates/harlo-core/src/phone/) for the iOS app. It builds on Linux
// too, so tools/ios-e2e.sh can test it against the real harlo.
import PackageDescription

let package = Package(
    name: "HarloKit",
    platforms: [.iOS(.v18), .macOS(.v14)],
    products: [
        .library(name: "HarloKit", targets: ["HarloKit"]),
    ],
    targets: [
        .target(name: "CTweetNaCl"),
        .target(name: "HarloKit", dependencies: ["CTweetNaCl"]),
        // Pairs with a running harlo and drives a session (tools/ios-e2e.sh).
        .executableTarget(name: "harlo-probe", dependencies: ["HarloKit"]),
        // Themes/: the desktop themes' phone palettes, read from disk.
        .testTarget(name: "HarloKitTests", dependencies: ["HarloKit"], exclude: ["Themes"]),
    ],
    swiftLanguageModes: [.v6]
)
