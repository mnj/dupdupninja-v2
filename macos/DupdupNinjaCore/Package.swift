// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "DupdupNinjaCore",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .library(name: "DupdupNinjaCore", targets: ["DupdupNinjaCore"]),
        .executable(name: "DupdupNinjaApp", targets: ["DupdupNinjaApp"]),
    ],
    targets: [
        .target(
            name: "CDupdupNinja",
            path: "Sources/CDupdupNinja",
            publicHeadersPath: "include"
        ),
        .target(
            name: "DupdupNinjaCore",
            dependencies: ["CDupdupNinja"],
            path: "Sources/DupdupNinjaCore"
        ),
        .executableTarget(
            name: "DupdupNinjaApp",
            dependencies: ["DupdupNinjaCore"],
            path: "Sources/DupdupNinjaApp",
            resources: [
                .process("Resources")
            ]
        ),
    ]
)
