// swift-tools-version: 6.0
import PackageDescription

/* name: "Commented" */
let helper = Target.target(name: "Helper")
let package = Package(
    name: "NativeSwift",
    targets: [helper]
)
