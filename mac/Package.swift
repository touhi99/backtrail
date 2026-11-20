// swift-tools-version: 6.0
import PackageDescription
let package = Package(
    name: "Backtrail",
    platforms: [.macOS(.v14)],
    targets: [.executableTarget(name: "Backtrail", path: "Sources/Backtrail", swiftSettings: [.swiftLanguageMode(.v5)])]
)
