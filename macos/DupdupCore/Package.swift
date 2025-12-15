// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "DupdupCore",
    products: [
        .library(name: "DupdupCore", targets: ["DupdupCore"]),
    ],
    targets: [
        .target(
            name: "CDupdup",
            path: "Sources/CDupdup",
            publicHeadersPath: "include"
        ),
        .target(
            name: "DupdupCore",
            dependencies: ["CDupdup"],
            path: "Sources/DupdupCore"
        ),
    ]
)

