// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "DupdupNinjaCore",
    products: [
        .library(name: "DupdupNinjaCore", targets: ["DupdupNinjaCore"]),
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
    ]
)
