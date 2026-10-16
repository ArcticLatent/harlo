// swift-tools-version:6.0
// HarloKit: the phone protocol (crates/harlo-core/src/phone.rs,
// crates/harlo-app/src/phone_server.rs) for the iOS app. It builds on Linux
// too, so scripts/ios-e2e.sh can test it against the real harlo.
import PackageDescription

let package = Package(
    name: "HarloKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "HarloKit", targets: ["HarloKit"]),
    ],
    targets: [
        .target(name: "CTweetNaCl"),
        .target(name: "HarloKit", dependencies: ["CTweetNaCl"]),
        // Pairs with a running harlo and drives a session (scripts/ios-e2e.sh).
        .executableTarget(name: "harlo-probe", dependencies: ["HarloKit"]),
        .testTarget(name: "HarloKitTests", dependencies: ["HarloKit"]),
    ],
    swiftLanguageModes: [.v5]
)
