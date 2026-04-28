// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "DevCleanerMac",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "DevCleanerMac", targets: ["DevCleanerMac"])
    ],
    targets: [
        .executableTarget(
            name: "DevCleanerMac",
            path: "Sources/DevCleanerMac",
            resources: [
                .copy("Resources")
            ]
        ),
        .testTarget(
            name: "DevCleanerMacTests",
            dependencies: ["DevCleanerMac"],
            path: "Tests/DevCleanerMacTests"
        )
    ]
)
